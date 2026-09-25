use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use futures::{SinkExt, StreamExt};
use music::cast::{Format as SinkFormat, Sink};
use tokio::sync::mpsc::UnboundedSender;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use super::clock::{self, Clock, Nudge, Sample};
use super::server::Playing;
use super::wire::{
    self, Codec, Farewell, FromHost, FromReceiver, HEADER, Header, MarkKind, PROTOCOL, Refusal,
};

const PROBE: Duration = Duration::from_secs(5);
const WELCOME_WAIT: Duration = Duration::from_secs(5);
const SLEW_GAP: Duration = Duration::from_millis(50);
const SINK_LATENCY: u32 = 60;
const MARGIN: u32 = 40;

#[derive(Debug)]
pub enum ReceiverEvent {
    Welcomed(String),
    Now(Playing),
    Refused(Refusal),
    Ended(Farewell),
    Lost,
}

pub async fn listen(
    at: String,
    code: String,
    name: String,
    events: UnboundedSender<ReceiverEvent>,
) {
    match join(&at, &code, &name, &events).await {
        Ok(()) => {}
        Err(error) => {
            log::warn!("jam: cannot follow {at}: {error:#}");
            events.send(ReceiverEvent::Lost).ok();
        }
    }
}

async fn join(
    at: &str,
    code: &str,
    name: &str,
    events: &UnboundedSender<ReceiverEvent>,
) -> Result<()> {
    let (mut socket, _) = connect_async(format!("ws://{at}/c/{code}/ws"))
        .await
        .context("cannot reach the host")?;

    let hello = FromReceiver::Hello {
        protocol: PROTOCOL,
        name: name.to_owned(),
        native: true,
        code: code.to_owned(),
        device: format!("{:032x}", fastrand::u128(..)),
        accepts: vec![Codec::Pcm16],
    };
    socket
        .send(Message::Text(wire::encode(&hello)?.into()))
        .await
        .context("cannot greet the host")?;

    let welcomed = tokio::time::timeout(WELCOME_WAIT, socket.next())
        .await
        .context("the host did not answer")?;
    let Some(Ok(Message::Text(line))) = welcomed else {
        bail!("the host closed before welcoming");
    };

    let (room, format, mut lead, mut origin) = match wire::decode::<FromHost>(&line)? {
        FromHost::Welcome {
            room,
            format,
            lead_ms,
            origin,
            ..
        } => (room, format, lead_ms, origin),
        FromHost::Refused { reason } => {
            events.send(ReceiverEvent::Refused(reason)).ok();
            return Ok(());
        }
        _ => bail!("the host spoke out of turn"),
    };

    let sink = Sink::open(SinkFormat {
        rate: format.rate,
        channels: format.channels,
    })
    .context("cannot open the receiving output")?;
    events.send(ReceiverEvent::Welcomed(room)).ok();

    let mut clock = Clock::new();
    let mut probe = tokio::time::interval(PROBE);
    let mut pushed = 0u64;
    let mut quiet_at: Option<u64> = None;
    let mut slewed = tokio::time::Instant::now();

    loop {
        tokio::select! {
            _ = probe.tick() => {
                let need = SINK_LATENCY + (clock.rtt() / 2) as u32 + MARGIN;
                let ping = FromReceiver::Ping { t0: millis(), need };
                if socket.send(Message::Text(wire::encode(&ping)?.into())).await.is_err() {
                    break;
                }
            }
            message = socket.next() => match message {
                Some(Ok(Message::Binary(frame))) => {
                    let header = Header::read(&frame)?;
                    if quiet_at.is_some_and(|at| header.first_sample >= at) {
                        continue;
                    }

                    let samples = pcm(&frame[HEADER..]);
                    let frames = samples.len() / format.channels.max(1) as usize;
                    sink.push(&samples);
                    pushed = pushed.saturating_add(frames as u64);

                    let due = clock::play_at(origin, header.first_sample, format.rate, lead) as i64
                        - clock.offset();
                    let waiting = (pushed.saturating_sub(sink.played()) * 1_000
                        / format.rate.max(1) as u64) as i64;
                    let error = millis() as i64 + waiting - due;

                    match clock::correction(error) {
                        Nudge::Hold => {}
                        Nudge::Slew(frames) => {
                            if slewed.elapsed() >= SLEW_GAP {
                                sink.nudge(frames);
                                slewed = tokio::time::Instant::now();
                            }
                        }
                        Nudge::Anchor => {
                            sink.flush();
                            pushed = sink.played();
                        }
                    }
                }
                Some(Ok(Message::Text(line))) => match wire::decode::<FromHost>(&line)? {
                    FromHost::Mark { mark: MarkKind::Quiet, at, origin: anchor } => {
                        origin = anchor;
                        quiet_at = Some(at);
                    }
                    FromHost::Mark { mark: MarkKind::Cut, origin: anchor, .. } => {
                        origin = anchor;
                        quiet_at = None;
                        sink.flush();
                        pushed = sink.played();
                    }
                    FromHost::Now {
                        title,
                        artist,
                        album,
                        cover,
                        duration_ms,
                        provider,
                    } => {
                        events
                            .send(ReceiverEvent::Now(Playing {
                                title,
                                artist,
                                album,
                                cover,
                                duration_ms,
                                provider,
                            }))
                            .ok();
                    }
                    FromHost::Lead { lead_ms } => lead = lead_ms,
                    FromHost::Transport { .. }
                    | FromHost::Found { .. }
                    | FromHost::Added { .. }
                    | FromHost::Denied { .. }
                    | FromHost::Grant { .. }
                    | FromHost::Controls { .. }
                    | FromHost::Lineup { .. }
                    | FromHost::Packs { .. }
                    | FromHost::Opened { .. }
                    | FromHost::Words { .. }
                    | FromHost::Room { .. } => continue,
                    FromHost::Pong { t0, t1, t2 } => {
                        clock.push(Sample { t0, t1, t2, t3: millis() });
                    }
                    FromHost::Ended { reason } => {
                        events.send(ReceiverEvent::Ended(reason)).ok();
                        break;
                    }
                    FromHost::Refused { reason } => {
                        events.send(ReceiverEvent::Refused(reason)).ok();
                        break;
                    }
                    FromHost::Welcome { .. } => continue,
                },
                Some(Ok(Message::Close(_))) | None => {
                    events.send(ReceiverEvent::Lost).ok();
                    break;
                }
                Some(Err(error)) => {
                    log::debug!("jam: the host connection failed: {error}");
                    events.send(ReceiverEvent::Lost).ok();
                    break;
                }
                _ => continue,
            },
        }
    }

    socket
        .send(Message::Text(wire::encode(&FromReceiver::Bye)?.into()))
        .await
        .ok();
    Ok(())
}

fn pcm(bytes: &[u8]) -> Vec<i16> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| i16::from_le_bytes(*pair))
        .collect()
}

fn millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}
