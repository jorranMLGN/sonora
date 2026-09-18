use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr, TcpListener as StdListener};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result};
use futures::{SinkExt, StreamExt};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full, StreamBody};
use hyper::body::{Bytes, Frame, Incoming};
use hyper::header::{CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, UPGRADE};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::sync::oneshot;
use tokio::sync::watch;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::Role;

use super::Listener;
use super::cast::Broadcast;
use super::wire::{
    self, Codec, Denial, Farewell, FromHost, FromReceiver, HEADER, Hit, MarkKind, PROTOCOL, Refusal,
};

const PAGE: &str = include_str!("page.html");
const STRIKES: usize = 5;
const WINDOW: Duration = Duration::from_secs(60);
const LISTENERS: usize = 8;
const HELLO_WAIT: Duration = Duration::from_secs(5);
const ASK_WAIT: Duration = Duration::from_secs(10);
const ADDS: usize = 50;
const FORMAT_WAIT: Duration = Duration::from_secs(120);
const WAV_HEADER: usize = 44;
const ENDLESS: u32 = u32::MAX;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Playing {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub cover: Option<String>,
    pub duration_ms: u64,
    pub provider: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Transport {
    pub playing: bool,
    pub position_ms: u64,
}

#[derive(Debug)]
pub enum ServerEvent {
    Joined(Listener),
    Left(String),
    Find {
        query: String,
        reply: oneshot::Sender<Vec<Hit>>,
    },
    Add {
        id: String,
        reply: oneshot::Sender<Option<String>>,
    },
}

pub struct Serving {
    pub kicks: broadcast::Sender<String>,
    pub code: String,
    pub room: String,
    pub lead: u32,
    pub broadcast: Arc<Broadcast>,
    pub now: watch::Receiver<Option<Playing>>,
    pub transport: watch::Receiver<Transport>,
    pub events: UnboundedSender<ServerEvent>,
}

struct Shared {
    serving: Serving,
    limits: Mutex<HashMap<IpAddr, (usize, Instant)>>,
    listeners: AtomicUsize,
    page: String,
}

pub fn bind(port: u16) -> Result<(StdListener, u16)> {
    let listener = match StdListener::bind(("0.0.0.0", port)) {
        Ok(listener) => listener,
        Err(_) => StdListener::bind(("0.0.0.0", 0)).context("cannot bind a jam port")?,
    };
    listener
        .set_nonblocking(true)
        .context("cannot make the jam listener non-blocking")?;
    let bound = listener
        .local_addr()
        .context("cannot read the jam port")?
        .port();
    Ok((listener, bound))
}

pub async fn run(listener: StdListener, serving: Serving) {
    let listener = match TcpListener::from_std(listener) {
        Ok(listener) => listener,
        Err(error) => return log::error!("jam: cannot take over the listener: {error}"),
    };

    let shared = Arc::new(Shared {
        page: rendered(),
        serving,
        limits: Mutex::new(HashMap::new()),
        listeners: AtomicUsize::new(0),
    });

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) => {
                log::warn!("jam: cannot accept a connection: {error}");
                continue;
            }
        };

        let shared = shared.clone();
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let service = service_fn(move |req| answer(req, peer, shared.clone()));
            if let Err(error) = http1::Builder::new()
                .serve_connection(io, service)
                .with_upgrades()
                .await
            {
                log::debug!("jam: connection ended: {error}");
            }
        });
    }
}

type Body = BoxBody<Bytes, Infallible>;

async fn answer(
    mut req: Request<Incoming>,
    peer: SocketAddr,
    shared: Arc<Shared>,
) -> Result<Response<Body>, Infallible> {
    if blocked(&shared, peer.ip()) {
        return Ok(refuse(StatusCode::TOO_MANY_REQUESTS));
    }

    let path = req.uri().path().to_owned();
    let page = format!("/c/{}", shared.serving.code);
    let socket = format!("{page}/ws");
    let stream = format!("{page}/stream.wav");

    if req.method() == Method::GET && path == page {
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/html; charset=utf-8")
            .body(boxed(Bytes::from(shared.page.clone())))
            .unwrap_or_else(|_| refuse(StatusCode::INTERNAL_SERVER_ERROR)));
    }

    if req.method() == Method::GET && path == stream {
        let Some(format) = awaited(&shared).await else {
            return Ok(refuse(StatusCode::SERVICE_UNAVAILABLE));
        };
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "audio/wav")
            .header("cache-control", "no-store")
            .body(wav(&shared, format))
            .unwrap_or_else(|_| refuse(StatusCode::INTERNAL_SERVER_ERROR)));
    }

    if path == socket && upgrading(&req) {
        let Some(key) = req.headers().get(SEC_WEBSOCKET_KEY).cloned() else {
            return Ok(refuse(StatusCode::BAD_REQUEST));
        };
        let accept = derive_accept_key(key.as_bytes());
        let taken = shared.clone();
        tokio::spawn(async move {
            match hyper::upgrade::on(&mut req).await {
                Ok(upgraded) => {
                    let stream = WebSocketStream::from_raw_socket(
                        TokioIo::new(upgraded),
                        Role::Server,
                        None,
                    )
                    .await;
                    session(stream, taken, peer).await;
                }
                Err(error) => log::debug!("jam: cannot upgrade: {error}"),
            }
        });

        return Ok(Response::builder()
            .status(StatusCode::SWITCHING_PROTOCOLS)
            .header(CONNECTION, "Upgrade")
            .header(UPGRADE, "websocket")
            .header(SEC_WEBSOCKET_ACCEPT, accept)
            .body(boxed(Bytes::new()))
            .unwrap_or_else(|_| refuse(StatusCode::INTERNAL_SERVER_ERROR)));
    }

    strike(&shared, peer.ip());
    Ok(refuse(StatusCode::NOT_FOUND))
}

async fn session(
    mut socket: WebSocketStream<TokioIo<hyper::upgrade::Upgraded>>,
    shared: Arc<Shared>,
    peer: SocketAddr,
) {
    let at = peer.to_string();
    let Some(hello) = greeted(&mut socket).await else {
        return;
    };
    let FromReceiver::Hello {
        protocol,
        name,
        native,
        accepts,
        ..
    } = hello
    else {
        return;
    };

    if protocol != PROTOCOL {
        return turn_down(&mut socket, Refusal::Protocol).await;
    }
    if !accepts.contains(&Codec::Pcm16) {
        return turn_down(&mut socket, Refusal::Codec).await;
    }
    if shared.listeners.load(Ordering::Acquire) >= LISTENERS {
        return turn_down(&mut socket, Refusal::Full).await;
    }
    let Some(format) = awaited(&shared).await else {
        return turn_down(&mut socket, Refusal::Closed).await;
    };

    let welcome = FromHost::Welcome {
        protocol: PROTOCOL,
        room: shared.serving.room.clone(),
        format,
        lead_ms: shared.serving.lead,
        origin: shared.serving.broadcast.origin(),
    };
    if say(&mut socket, &welcome).await.is_err() {
        return;
    }

    shared.listeners.fetch_add(1, Ordering::AcqRel);
    shared
        .serving
        .events
        .send(ServerEvent::Joined(Listener {
            name: name.clone(),
            at: at.clone(),
            native,
        }))
        .ok();

    let (outbox, mut waiting) = mpsc::unbounded_channel::<FromHost>();
    let mut added = 0usize;
    let mut chunks = shared.serving.broadcast.subscribe();
    let mut kicks = shared.serving.kicks.subscribe();
    let mut now = shared.serving.now.clone();
    let mut transport = shared.serving.transport.clone();
    let mut frame: Vec<u8> = Vec::new();

    loop {
        tokio::select! {
            chunk = chunks.recv() => match chunk {
                Ok(chunk) => {
                    if let Some(mark) = chunk.mark {
                        let mark = FromHost::Mark { mark, at: chunk.header.first_sample };
                        if say(&mut socket, &mark).await.is_err() {
                            break;
                        }
                    }
                    if chunk.bytes.is_empty() {
                        continue;
                    }

                    frame.clear();
                    frame.reserve(HEADER + chunk.bytes.len());
                    chunk.header.write(&mut frame);
                    frame.extend_from_slice(&chunk.bytes);
                    if socket.send(Message::Binary(frame.clone().into())).await.is_err() {
                        break;
                    }
                }
                Err(RecvError::Lagged(missed)) => {
                    log::debug!("jam: a receiver fell {missed} chunks behind");
                    let cut = FromHost::Mark { mark: MarkKind::Cut, at: 0 };
                    if say(&mut socket, &cut).await.is_err() {
                        break;
                    }
                }
                Err(RecvError::Closed) => break,
            },
            moved = transport.changed() => {
                if moved.is_err() {
                    break;
                }
                let seen = *transport.borrow_and_update();
                let told = FromHost::Transport {
                    playing: seen.playing,
                    position_ms: seen.position_ms,
                };
                if say(&mut socket, &told).await.is_err() {
                    break;
                }
            },
            changed = now.changed() => {
                if changed.is_err() {
                    break;
                }
                let playing = now.borrow_and_update().clone();
                if let Some(playing) = playing {
                    let told = FromHost::Now {
                        title: playing.title,
                        artist: playing.artist,
                        album: playing.album,
                        cover: playing.cover,
                        duration_ms: playing.duration_ms,
                        provider: playing.provider,
                    };
                    if say(&mut socket, &told).await.is_err() {
                        break;
                    }
                }
            },
            told = waiting.recv() => match told {
                Some(message) => {
                    if say(&mut socket, &message).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
            kicked = kicks.recv() => match kicked {
                Ok(target) if target == at => {
                    let ended = FromHost::Ended { reason: Farewell::Kicked };
                    say(&mut socket, &ended).await.ok();
                    socket.close(None).await.ok();
                    break;
                }
                Err(RecvError::Closed) => break,
                _ => continue,
            },
            message = socket.next() => match message {
                Some(Ok(Message::Text(line))) => {
                    let t1 = millis();
                    match wire::decode::<FromReceiver>(&line) {
                        Ok(FromReceiver::Ping { t0 }) => {
                            let pong = FromHost::Pong { t0, t1, t2: millis() };
                            if say(&mut socket, &pong).await.is_err() {
                                break;
                            }
                        }
                        Ok(FromReceiver::Find { query }) => {
                            ask(&shared, &outbox, Asked::Find(query));
                        }
                        Ok(FromReceiver::Add { id }) => {
                            added += 1;
                            match added > ADDS {
                                true => {
                                    let denied = FromHost::Denied { reason: Denial::Busy };
                                    outbox.send(denied).ok();
                                }
                                false => ask(&shared, &outbox, Asked::Add(id)),
                            }
                        }
                        Ok(FromReceiver::Bye) => break,
                        _ => continue,
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Err(_)) => break,
                _ => continue,
            },
        }
    }

    shared.listeners.fetch_sub(1, Ordering::AcqRel);
    shared.serving.events.send(ServerEvent::Left(at)).ok();
}

async fn awaited(shared: &Arc<Shared>) -> Option<super::wire::Format> {
    if let Some(format) = shared.serving.broadcast.format() {
        return Some(format);
    }

    let mut formats = shared.serving.broadcast.formats();
    let waited = tokio::time::timeout(FORMAT_WAIT, async {
        loop {
            if formats.changed().await.is_err() {
                return None;
            }
            if let Some(format) = *formats.borrow_and_update() {
                return Some(format);
            }
        }
    })
    .await;

    waited.ok().flatten()
}

enum Asked {
    Find(String),
    Add(String),
}

fn ask(shared: &Arc<Shared>, outbox: &UnboundedSender<FromHost>, asked: Asked) {
    let events = shared.serving.events.clone();
    let outbox = outbox.clone();

    tokio::spawn(async move {
        match asked {
            Asked::Find(query) => {
                let (reply, answer) = oneshot::channel();
                let sent = ServerEvent::Find {
                    query: query.clone(),
                    reply,
                };
                if events.send(sent).is_err() {
                    return;
                }
                let hits = match tokio::time::timeout(ASK_WAIT, answer).await {
                    Ok(Ok(hits)) => hits,
                    _ => Vec::new(),
                };
                outbox.send(FromHost::Found { query, hits }).ok();
            }
            Asked::Add(id) => {
                let (reply, answer) = oneshot::channel();
                if events.send(ServerEvent::Add { id, reply }).is_err() {
                    return;
                }
                let told = match tokio::time::timeout(ASK_WAIT, answer).await {
                    Ok(Ok(Some(title))) => FromHost::Added { title },
                    Ok(Ok(None)) => FromHost::Denied {
                        reason: Denial::Unknown,
                    },
                    _ => FromHost::Denied {
                        reason: Denial::Busy,
                    },
                };
                outbox.send(told).ok();
            }
        }
    });
}

async fn greeted(
    socket: &mut WebSocketStream<TokioIo<hyper::upgrade::Upgraded>>,
) -> Option<FromReceiver> {
    let waited = tokio::time::timeout(HELLO_WAIT, socket.next()).await.ok()?;
    let Some(Ok(Message::Text(line))) = waited else {
        return None;
    };
    wire::decode::<FromReceiver>(&line).ok()
}

async fn say(
    socket: &mut WebSocketStream<TokioIo<hyper::upgrade::Upgraded>>,
    message: &FromHost,
) -> Result<()> {
    let line = wire::encode(message)?;
    socket
        .send(Message::Text(line.into()))
        .await
        .context("cannot write to a receiver")
}

async fn turn_down(
    socket: &mut WebSocketStream<TokioIo<hyper::upgrade::Upgraded>>,
    reason: Refusal,
) {
    say(socket, &FromHost::Refused { reason }).await.ok();
    socket.close(None).await.ok();
}

fn upgrading(req: &Request<Incoming>) -> bool {
    req.headers()
        .get(UPGRADE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
}

fn blocked(shared: &Shared, peer: IpAddr) -> bool {
    let mut limits = match shared.limits.lock() {
        Ok(limits) => limits,
        Err(poisoned) => poisoned.into_inner(),
    };
    match limits.get(&peer) {
        Some((count, since)) if *count >= STRIKES && since.elapsed() < WINDOW => true,
        Some((_, since)) if since.elapsed() >= WINDOW => {
            limits.remove(&peer);
            false
        }
        _ => false,
    }
}

fn strike(shared: &Shared, peer: IpAddr) {
    let mut limits = match shared.limits.lock() {
        Ok(limits) => limits,
        Err(poisoned) => poisoned.into_inner(),
    };
    let entry = limits.entry(peer).or_insert((0, Instant::now()));
    if entry.1.elapsed() >= WINDOW {
        *entry = (0, Instant::now());
    }
    entry.0 += 1;
}

fn refuse(status: StatusCode) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(boxed(Bytes::from_static(b"no\n")))
        .unwrap_or_else(|_| Response::new(boxed(Bytes::new())))
}

fn boxed(bytes: Bytes) -> Body {
    Full::new(bytes).boxed()
}

fn wav(shared: &Arc<Shared>, format: wire::Format) -> Body {
    let head = header(format);
    let chunks = shared.serving.broadcast.subscribe();
    let sound = futures::stream::unfold(chunks, |mut chunks| async move {
        loop {
            match chunks.recv().await {
                Ok(chunk) if chunk.bytes.is_empty() => continue,
                Ok(chunk) => {
                    let frame = Frame::data(Bytes::from(chunk.bytes.clone()));
                    return Some((Ok(frame), chunks));
                }
                Err(RecvError::Lagged(missed)) => {
                    log::debug!("jam: a stream fell {missed} chunks behind");
                }
                Err(RecvError::Closed) => return None,
            }
        }
    });

    let opening = futures::stream::once(async move { Ok(Frame::data(Bytes::from(head))) });
    BodyExt::boxed(StreamBody::new(opening.chain(sound)))
}

fn header(format: wire::Format) -> Vec<u8> {
    let channels = format.channels.max(1);
    let rate = format.rate.max(1);
    let bytes_per_frame = channels as u32 * 2;

    let mut head = Vec::with_capacity(WAV_HEADER);
    head.extend_from_slice(b"RIFF");
    head.extend_from_slice(&ENDLESS.to_le_bytes());
    head.extend_from_slice(b"WAVEfmt ");
    head.extend_from_slice(&16u32.to_le_bytes());
    head.extend_from_slice(&1u16.to_le_bytes());
    head.extend_from_slice(&channels.to_le_bytes());
    head.extend_from_slice(&rate.to_le_bytes());
    head.extend_from_slice(&(rate * bytes_per_frame).to_le_bytes());
    head.extend_from_slice(&(bytes_per_frame as u16).to_le_bytes());
    head.extend_from_slice(&16u16.to_le_bytes());
    head.extend_from_slice(b"data");
    head.extend_from_slice(&ENDLESS.to_le_bytes());
    head
}

fn rendered() -> String {
    let mut out = String::with_capacity(PAGE.len());
    let mut rest = PAGE;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str("{{");
            rest = after;
            continue;
        };

        let key = after[..end].trim();
        match key {
            "protocol" => out.push_str(&PROTOCOL.to_string()),
            key => out.push_str(&i18n::lookup(key, None)),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

fn millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}
