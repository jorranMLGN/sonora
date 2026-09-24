use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

#[cfg(test)]
use super::auth::ClientId;
use super::http::Http;
use super::stream;
use super::wire::{self, Transcoding};
use crate::audio::trim;
use crate::audio::{Output, RAMP, SmoothGain, Trimmed, Volume};
use crate::cast::CastSink;
use crate::spectrum::Spectrum;
use crate::{PlaybackConfig, PlaybackEvent, PlaybackEvents, PlaybackFactory, Player};

const POLL: Duration = Duration::from_millis(20);

/// Ranks a transcoding preset; higher is better. Unknown presets rank above
/// nothing but below anything named, so a preset SoundCloud adds later is
/// still playable rather than silently skipped.
fn rank(preset: &str) -> u8 {
    match preset {
        "aac_160k" => 4,
        "aac_96k" => 2,
        "mp3_0_0" => 1,
        _ => 1,
    }
}

/// Chooses which transcoding to play.
///
/// The only `progressive` transcoding soundcloud offers is a legacy 128kbps
/// mp3; every better encoding (`aac_160k`, `aac_96k`) is hls-only. So this
/// prefers the best-ranked hls entry and falls back to progressive only when
/// no hls entry is offered at all. Filtering happens on `format.protocol`,
/// never on `preset` alone: `mp3_0_0` exists as both an hls and a progressive
/// entry.
///
/// An `abr_` preset is never chosen. It names an adaptive master playlist of
/// variants, which `stream::assemble` cannot read, and soundcloud refuses to
/// resolve it with a 404 anyway — so ranking it would only fail a track that
/// has a playable `aac_96k` beside it.
pub fn pick(transcodings: &[Transcoding]) -> Option<&Transcoding> {
    transcodings
        .iter()
        .filter(|t| t.format.protocol == "hls" && !t.preset.starts_with("abr_"))
        .max_by_key(|t| rank(&t.preset))
        .or_else(|| {
            transcodings
                .iter()
                .find(|t| t.format.protocol == "progressive")
        })
}

enum Command {
    Load { id: String, at: Option<Duration> },
    Preload { id: String },
    Play,
    Pause,
    Seek(Duration),
    Gain(f32),
}

pub struct Factory {
    http: Arc<Http>,
}

impl Factory {
    pub fn new(http: Arc<Http>) -> Self {
        Self { http }
    }
}

impl PlaybackFactory for Factory {
    fn start(
        &self,
        config: PlaybackConfig,
        cast: Option<CastSink>,
    ) -> (Box<dyn Player>, Box<dyn PlaybackEvents>) {
        let (commands, command_rx) = unbounded_channel();
        let (events, event_rx) = unbounded_channel();
        let http = self.http.clone();
        let spectrum = Spectrum::new();
        let engine_spectrum = spectrum.clone();
        let spawned = std::thread::Builder::new()
            .name("sc-playback".to_string())
            .spawn(move || run(http, config, command_rx, events, engine_spectrum, cast));
        if let Err(error) = spawned {
            log::error!("playback: cannot spawn engine thread: {error}");
        }
        (
            Box::new(Engine { commands, spectrum }),
            Box::new(Events(event_rx)),
        )
    }
}

struct Engine {
    commands: UnboundedSender<Command>,
    spectrum: Spectrum,
}

impl Player for Engine {
    fn load(&self, track_id: &str, _seamless: bool) -> Result<()> {
        self.commands
            .send(Command::Load {
                id: track_id.to_string(),
                at: None,
            })
            .context("cannot reach playback engine")
    }

    fn load_paused_at(&self, track_id: &str, at: Duration) -> Result<()> {
        self.commands
            .send(Command::Load {
                id: track_id.to_string(),
                at: Some(at),
            })
            .context("cannot reach playback engine")
    }

    fn preload(&self, track_id: &str) -> Result<()> {
        self.commands
            .send(Command::Preload {
                id: track_id.to_string(),
            })
            .context("cannot reach playback engine")
    }

    fn play(&self) {
        self.commands.send(Command::Play).ok();
    }

    fn pause(&self) {
        self.commands.send(Command::Pause).ok();
    }

    fn seek(&self, position: Duration) {
        self.commands.send(Command::Seek(position)).ok();
    }

    fn set_gain(&self, gain: f32) {
        self.commands.send(Command::Gain(gain)).ok();
    }

    fn spectrum(&self) -> Option<Spectrum> {
        Some(self.spectrum.clone())
    }
}

pub struct Events(UnboundedReceiver<PlaybackEvent>);

#[async_trait]
impl PlaybackEvents for Events {
    async fn next(&mut self) -> Option<PlaybackEvent> {
        self.0.recv().await
    }
}

#[derive(Clone)]
struct Loaded {
    data: Arc<Vec<u8>>,
    duration: Option<Duration>,
}

struct Slot {
    id: String,
    length: Option<Duration>,
    envelope: Volume,
    gain: f32,
}

impl Slot {
    fn mute(&self) {
        self.envelope.set(0.0);
    }

    fn unmute(&self) {
        self.envelope.set(self.gain);
    }
}

enum Kind {
    Play,
    Ahead,
}

struct Fetched {
    epoch: u64,
    id: String,
    kind: Kind,
    result: Result<Loaded>,
}

fn run(
    http: Arc<Http>,
    config: PlaybackConfig,
    commands: UnboundedReceiver<Command>,
    events: UnboundedSender<PlaybackEvent>,
    spectrum: Spectrum,
    cast: Option<CastSink>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            log::error!("playback: cannot build engine runtime: {error}");
            return;
        }
    };
    runtime.block_on(engine_loop(http, config, commands, events, spectrum, cast));
}

async fn engine_loop(
    http: Arc<Http>,
    config: PlaybackConfig,
    mut commands: UnboundedReceiver<Command>,
    events: UnboundedSender<PlaybackEvent>,
    spectrum: Spectrum,
    cast: Option<CastSink>,
) {
    let output = match Output::open(Volume::new(config.gain), spectrum, "soundcloud", cast) {
        Ok(output) => output,
        Err(error) => {
            log::error!("playback: cannot open audio output: {error:#}");
            return;
        }
    };
    let sink = output.sink().clone();
    sink.pause();

    let (fetched, mut arrivals) = unbounded_channel::<Fetched>();
    let mut ticker = tokio::time::interval(POLL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let report_every = (config.position_interval.as_millis() / POLL.as_millis()).max(1) as u32;
    let mut ticks = 0u32;
    let mut output_ticks = 0u32;

    let mut playing = false;
    let mut autostart = true;
    let mut hold: Option<Duration> = None;
    let mut epoch = 0u64;
    let mut pending: Option<u64> = None;
    let mut inflight: Option<tokio::task::AbortHandle> = None;
    let mut current: Option<Slot> = None;
    let mut queued: Option<Slot> = None;
    let mut ahead: Option<(String, Loaded)> = None;
    let mut prev_len = 0usize;

    loop {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else { break };
                match command {
                    Command::Load { id, at } => {
                        if at.is_none() && current.as_ref().is_some_and(|slot| slot.id == id) {
                            playing = true;
                            autostart = true;
                            if let Some(slot) = &current {
                                slot.unmute();
                                if let Some(length) = slot.length {
                                    events.send(PlaybackEvent::Length(length)).ok();
                                }
                            }
                            sink.play();
                            events.send(PlaybackEvent::Playing(sink.get_pos())).ok();
                            continue;
                        }
                        epoch += 1;
                        if let Some(handle) = inflight.take() {
                            handle.abort();
                        }
                        let cached = ahead
                            .take_if(|(cached, _)| *cached == id)
                            .map(|(_, loaded)| loaded);
                        pending = None;
                        if cached.is_none() {
                            ahead = None;
                            pending = Some(epoch);
                            inflight =
                                Some(spawn(&http, id.clone(), epoch, Kind::Play, &fetched));
                        }
                        events.send(PlaybackEvent::Loading(at.unwrap_or_default())).ok();
                        if output.failed() || output.changed() {
                            events.send(PlaybackEvent::OutputChanged).ok();
                            return;
                        }
                        silence(&sink, current.as_ref()).await;
                        current = None;
                        queued = None;
                        playing = false;
                        autostart = at.is_none();
                        hold = at;
                        prev_len = 0;
                        let Some(loaded) = cached else { continue };
                        match begin(&sink, &id, &loaded, autostart, hold.take()) {
                            Ok(slot) => {
                                announce(&events, &slot, autostart, at.unwrap_or_default());
                                prev_len = sink.len();
                                current = Some(slot);
                                playing = autostart;
                            }
                            Err(error) => {
                                log::warn!("playback: cannot decode {id}: {error:#}");
                                events.send(PlaybackEvent::Unavailable).ok();
                            }
                        }
                    }
                    Command::Preload { id } => {
                        let known = current.as_ref().is_some_and(|slot| slot.id == id)
                            || queued.as_ref().is_some_and(|slot| slot.id == id)
                            || ahead.as_ref().is_some_and(|(cached, _)| *cached == id);
                        if known || current.is_none() {
                            continue;
                        }
                        spawn(&http, id, epoch, Kind::Ahead, &fetched);
                    }
                    Command::Play => {
                        autostart = true;
                        if output.failed() || output.changed() {
                            events.send(PlaybackEvent::OutputChanged).ok();
                            return;
                        }
                        if let Some(slot) = &current {
                            sink.play();
                            slot.unmute();
                            playing = true;
                            events.send(PlaybackEvent::Playing(sink.get_pos())).ok();
                        }
                    }
                    Command::Pause => {
                        autostart = false;
                        playing = false;
                        let position = sink.get_pos();
                        if let Some(slot) = &current {
                            slot.mute();
                            await_drain(&sink).await;
                            sink.pause();
                        }
                        events.send(PlaybackEvent::Paused(position)).ok();
                    }
                    Command::Seek(position) => match &current {
                        None if hold.is_some() => hold = Some(position),
                        None => {}
                        Some(slot) => {
                            slot.mute();
                            await_drain(&sink).await;
                            if let Err(error) = sink.try_seek(position) {
                                log::warn!("playback: cannot seek: {error}");
                            }
                            if playing {
                                slot.unmute();
                            }
                            events.send(PlaybackEvent::Position(sink.get_pos())).ok();
                        }
                    },
                    Command::Gain(level) => output.set_volume(level),
                }
            }
            arrival = arrivals.recv() => {
                let Some(Fetched { epoch: at, id, kind, result }) = arrival else { break };
                if at != epoch {
                    continue;
                }
                match kind {
                    Kind::Play => {
                        if pending != Some(at) {
                            continue;
                        }
                        pending = None;
                        inflight = None;
                        let at = hold.take();
                        match result
                            .and_then(|loaded| begin(&sink, &id, &loaded, autostart, at))
                        {
                            Ok(slot) => {
                                announce(&events, &slot, autostart, at.unwrap_or_default());
                                prev_len = sink.len();
                                current = Some(slot);
                                playing = autostart;
                            }
                            Err(error) => {
                                log::warn!("playback: cannot load {id}: {error:#}");
                                events.send(PlaybackEvent::Unavailable).ok();
                            }
                        }
                    }
                    Kind::Ahead => {
                        let Ok(loaded) = result else {
                            continue;
                        };
                        if current.is_some() && queued.is_none() {
                            match append(&sink, &id, &loaded, false) {
                                Ok(slot) => {
                                    log::debug!("playback: {id} is queued for a gapless segue");
                                    queued = Some(slot);
                                    prev_len = sink.len();
                                }
                                Err(error) => {
                                    log::warn!("playback: cannot decode preload {id}: {error:#}")
                                }
                            }
                        }
                        ahead = Some((id, loaded));
                    }
                }
            }
            _ = ticker.tick() => {
                output_ticks += 1;
                if playing && (output.failed() || output_ticks >= report_every && output.changed()) {
                    events.send(PlaybackEvent::OutputChanged).ok();
                    return;
                }
                if output_ticks >= report_every {
                    output_ticks = 0;
                }
                let len = sink.len();
                ticks += 1;
                if current.is_some() && playing && len < prev_len {
                    ticks = 0;
                    events.send(PlaybackEvent::Ended).ok();
                    current = queued.take();
                    ahead = None;
                    playing = current.is_some();
                    match &current {
                        Some(slot) => {
                            if let Some(length) = slot.length {
                                events.send(PlaybackEvent::Length(length)).ok();
                            }
                            events.send(PlaybackEvent::Position(sink.get_pos())).ok();
                        }
                        None => log::debug!("playback: track ended with nothing queued ahead"),
                    }
                } else if playing && ticks >= report_every {
                    ticks = 0;
                    events.send(PlaybackEvent::Position(sink.get_pos())).ok();
                }
                prev_len = len;
            }
        }
    }
}

fn spawn(
    http: &Arc<Http>,
    id: String,
    epoch: u64,
    kind: Kind,
    fetched: &UnboundedSender<Fetched>,
) -> tokio::task::AbortHandle {
    let http = http.clone();
    let fetched = fetched.clone();
    tokio::spawn(async move {
        let result = fetch(&http, &id).await;
        fetched
            .send(Fetched {
                epoch,
                id,
                kind,
                result,
            })
            .ok();
    })
    .abort_handle()
}

async fn silence(sink: &rodio::Player, slot: Option<&Slot>) {
    let Some(slot) = slot else {
        sink.clear();
        return;
    };
    slot.mute();
    await_drain(sink).await;
    sink.clear();
}

async fn await_drain(sink: &rodio::Player) {
    if sink.is_paused() {
        return;
    }
    tokio::time::sleep(RAMP).await;
}

fn begin(
    sink: &rodio::Player,
    id: &str,
    loaded: &Loaded,
    start: bool,
    at: Option<Duration>,
) -> Result<Slot> {
    sink.clear();
    let slot = append(sink, id, loaded, true)?;
    if let Some(at) = at
        && let Err(error) = sink.try_seek(at)
    {
        log::warn!("playback: cannot start {id} at {}s: {error}", at.as_secs());
    }
    match start {
        true => sink.play(),
        false => sink.pause(),
    }
    Ok(slot)
}

fn append(sink: &rodio::Player, id: &str, loaded: &Loaded, fade: bool) -> Result<Slot> {
    // Unlike youtube::playback's `normalisation`, which attenuates by a
    // per-track `format.loudness_db`, the v2 api carries no per-track
    // loudness anywhere in its responses. There is nothing to compute a
    // per-track gain from, so this always plays at unity gain; only the
    // global `PlaybackConfig.gain`, applied once in `Output::open`, affects
    // volume here.
    let gain = 1.0;
    let envelope = Volume::new(gain);
    let initial = match fade {
        true => 0.0,
        false => gain,
    };
    let source = decode(loaded.data.clone())?;
    let edit = trim::from_mp4(&loaded.data);
    match edit {
        Some(edit) => log::debug!(
            "playback: {id} trims {:?} of priming, plays {:?}",
            edit.skip,
            edit.take
        ),
        None => log::debug!("playback: {id} carries no edit list"),
    }
    let source = Trimmed::new(
        source,
        edit.map(|edit| edit.skip).unwrap_or_default(),
        edit.and_then(|edit| edit.take),
    );
    sink.append(SmoothGain::new(source, envelope.clone(), initial, RAMP));
    Ok(Slot {
        id: id.to_string(),
        length: loaded.duration,
        envelope,
        gain,
    })
}

fn announce(
    events: &UnboundedSender<PlaybackEvent>,
    slot: &Slot,
    playing: bool,
    position: Duration,
) {
    if let Some(length) = slot.length {
        events.send(PlaybackEvent::Length(length)).ok();
    }
    let event = match playing {
        true => PlaybackEvent::Playing(position),
        false => PlaybackEvent::Paused(position),
    };
    events.send(event).ok();
}

/// Resolves a track id to its bytes: fetch the track for its transcodings,
/// `pick` the best one, resolve that transcoding's own url to the cdn url,
/// then download it.
///
/// `progressive` is a single GET. `hls` resolves to a media playlist url
/// instead of a direct one, so it goes through `stream::assemble`, which
/// fetches that playlist fresh, then the init segment and every media
/// segment in order, concatenating them into one fragmented mp4 buffer.
/// Fetches a track's transcodings, `pick`s the best one, and resolves that
/// transcoding's own url to the short-lived cdn (or, for hls, media
/// playlist) url named in `media.transcodings[].url`.
///
/// This is the whole selection step `fetch` needs before it downloads
/// anything, split out so `soundcloud::resolve_playable_url` — the entry
/// point `live_tests::soundcloud` calls to exercise this same path live —
/// can share it instead of reimplementing it.
async fn resolve_transcoding(
    http: &Http,
    id: &str,
) -> Result<(Transcoding, String, Option<Duration>)> {
    let track: wire::Track = http
        .get_json(&format!("/tracks/{id}"), &[])
        .await
        .context("cannot fetch the soundcloud track")?;
    let duration = Some(Duration::from_millis(track.duration));
    let chosen = pick(&track.media.transcodings)
        .context("soundcloud offers no playable transcoding for this track")?
        .clone();
    let resolved = http
        .resolve_stream(&chosen.url)
        .await
        .context("cannot resolve the soundcloud stream")?;
    Ok((chosen, resolved, duration))
}

/// Resolves a track id to the playable url `load` would stream from, without
/// downloading it — anonymous when `token` is `None`.
///
/// Exposed to the crate for `live_tests::soundcloud`, which needs this exact
/// selection-and-resolve path exercised against the live api rather than
/// reimplemented by hand, but cannot see `Http` or `Transcoding`: both stay
/// private to `soundcloud`, so this takes and returns plain strings.
///
/// `cfg(test)`: its only caller is a `#[tokio::test]`, which the compiler
/// already strips outside test builds, so this would otherwise warn as dead
/// code in a plain `cargo build`/`clippy`.
#[cfg(test)]
pub(crate) async fn resolve_playable_url(
    client_id: String,
    token: Option<String>,
    track_id: &str,
) -> Result<String> {
    let client_id = ClientId::new(
        client_id,
        std::env::temp_dir().join("sonora-test-client-id"),
    );
    let http = match token {
        Some(token) => Http::with_token(client_id, token),
        None => Http::anonymous(client_id),
    };
    let (_chosen, url, _duration) = resolve_transcoding(&http, track_id).await?;
    Ok(url)
}

async fn fetch(http: &Http, id: &str) -> Result<Loaded> {
    let (chosen, resolved, duration) = resolve_transcoding(http, id).await?;
    let data = match chosen.format.protocol.as_str() {
        "progressive" => http
            .fetch_bytes(&resolved)
            .await
            .context("cannot download the soundcloud stream")?,
        "hls" => stream::assemble(http.agent(), &resolved)
            .await
            .context("cannot assemble the soundcloud hls stream")?,
        other => anyhow::bail!("soundcloud offered an unrecognised stream protocol {other}"),
    };
    Ok(Loaded {
        data: Arc::new(data),
        duration,
    })
}

fn decode(data: Arc<Vec<u8>>) -> Result<impl rodio::Source + Send + 'static> {
    let length = data.len() as u64;
    rodio::Decoder::builder()
        .with_data(Cursor::new(Bytes(data)))
        .with_byte_len(length)
        .with_seekable(true)
        .build()
        .context("cannot decode audio")
}

struct Bytes(Arc<Vec<u8>>);

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::pick;
    use crate::soundcloud::wire::{Format, Track, Transcoding};

    fn transcoding(protocol: &str, preset: &str) -> Transcoding {
        Transcoding {
            url: format!("https://example.test/{preset}"),
            preset: preset.to_string(),
            format: Format {
                protocol: protocol.to_string(),
            },
            legacy: false,
        }
    }

    #[test]
    fn prefers_hls_over_progressive() {
        let all = vec![
            transcoding("progressive", "mp3_0_0"),
            transcoding("hls", "aac_160k"),
        ];
        assert_eq!(pick(&all).unwrap().format.protocol, "hls");
    }

    #[test]
    fn falls_back_to_progressive_when_no_hls_is_offered() {
        let all = vec![transcoding("progressive", "mp3_0_0")];
        assert_eq!(pick(&all).unwrap().format.protocol, "progressive");
    }

    #[test]
    fn has_nothing_to_pick_from_an_empty_list() {
        assert!(pick(&[]).is_none());
    }

    #[test]
    fn ranks_hls_presets_by_a_table_not_the_quality_label() {
        // aac_160k and abr_sq both carry the "sq" quality label, so a
        // heuristic on that label alone could not tell them apart.
        let all = vec![
            transcoding("hls", "abr_sq"),
            transcoding("hls", "aac_160k"),
            transcoding("hls", "aac_96k"),
        ];
        assert_eq!(pick(&all).unwrap().preset, "aac_160k");
    }

    #[test]
    fn filters_on_protocol_since_mp3_0_0_exists_as_both_hls_and_progressive() {
        let all = vec![
            transcoding("hls", "mp3_0_0"),
            transcoding("progressive", "mp3_0_0"),
        ];
        assert_eq!(pick(&all).unwrap().format.protocol, "hls");
    }

    #[test]
    fn an_unknown_preset_stays_playable_rather_than_being_skipped() {
        let all = vec![transcoding("hls", "some_future_preset")];
        assert!(pick(&all).is_some());
    }

    #[test]
    fn picks_the_best_transcoding_from_the_captured_fixture() {
        let track: Track =
            serde_json::from_str(include_str!("fixtures/track.json")).expect("fixture parses");
        let chosen = pick(&track.media.transcodings).expect("the fixture offers transcodings");
        assert_eq!(chosen.preset, "aac_160k");
        assert_eq!(chosen.format.protocol, "hls");
    }
}
