# Watching YouTube Video Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show a playing YouTube video in sync with its sound. It plays in the sidebar cover slot by default, fills the fullscreen view on demand, and can move to a floating always-on-top PiP window.

**Architecture:**
- **Fork:** the ytmusic fork learns to open and renew a video byte stream and to read a track's `musicVideoType`.
- **`crates/video`:** gains a streaming `Movie` player. It demuxes fragmented MP4 from any `Read + Seek` and decodes by presentation time against an external `Clock`.
- **`music`:** a new `MusicApi::video` hands out a ranged reader built on `music::stream`.
- **`state::Video`:** decides where the video plays, at what height, and when to stop.
- **`views` and `sonora`:** draw the video in the slot, in fullscreen and in a PiP window.

**Tech Stack:** Rust 2024, GPUI, tokio, `openh264` (libloading), `mp4`, `ytmusic` fork.

**Spec:** `docs/superpowers/specs/2026-10-20-youtube-video-design.md`

## Global Constraints

**Git and commits**
- **Never run `git commit` or `git push` without the user's explicit approval for that action.** This covers subagents too. Subagents stage only, and the controller commits. Ask before starting whether local commits on `feat/youtube-video` and the fork's `feat/video` may go ahead without asking each time.
- Commit messages are Conventional Commits: lowercase, imperative, no body. Never add `Co-Authored-By` or any assistant attribution.
- Never stage the root `Cargo.toml` or `Cargo.lock` while the local `[patch]` to `../ytmusic-rs` is in place (Tasks 0–7).

**Crate boundaries**
- `crates/video` knows nothing about music or gpui. `music::video` is the only door `state` and `views` use. `openh264` stays `libloading`-only, and OpenH264 is never bundled.
- `state` and `views` reach providers only through `MusicApi`. Network work goes inside `Io::global(cx).spawn`. A view never drives a player.

**UI and copy**
- Colors and sizes come from `cx.theme()` and `theme.metrics`. Breakpoints come from `ui::Room`. Every user-facing string lives in `assets/i18n/en-US/main.ftl`, and `t!` is never called in a constructor.
- Icons are added to `assets/icons/lucide/`, the fallback pack.

**Behaviour fixed by the spec**
- Places: `Slot` (sidebar Queue tab, default), `Fullscreen`, `Pip`. The video plays in one place at a time.
- Heights: `Slot` and `Pip` use 480p. `Fullscreen` uses 1080p. Both are capped by the setting "maximum video quality".
- No visible place means no video work. The grace period is 5 s, and no PiP opens by itself.
- YouTube Music shows video only for `musicVideoType` OMV or UGC, never ATV. One Music-client player request per track, made only when the video is wanted, and cached.
- Reduce motion does not turn video off. The video toggle defaults to on and is remembered.
- The PiP window is resizable and always on top. It remembers its size, and its corner is a setting.

**Tests**
- TDD. Tests go in `#[cfg(test)] mod tests` at the bottom of the file they cover.
- A test that needs OpenH264 returns early with an `eprintln!` when `video::codec::find()` is `None`.
- Clippy stays clean.

## Review Focus

1. **Seeking far ahead while the video shows.** The picture lands on the new position within about 2 s and never freezes on a stale frame. Tests: Task 2 `a_seek_lands_on_the_keyframe_before_the_target`; Task 4 `a_seek_resets_the_clock`.
2. **Changing track mid-video.** The previous video's frames never show on the next track. Test: Task 4 `a_new_track_drops_the_old_movie`.
3. **OpenH264 missing or declined.** The cover shows, with no repeated errors and no toast loop. Test: Task 4 `no_codec_means_no_video`.
4. **Pause.** The picture holds its frame and the decoder stops working. Test: Task 2 `a_paused_clock_holds_the_frame`.
5. **A stream url refused partway through a long set.** It renews and continues. Test: Task 1 `a_refused_video_stream_renews_by_its_itag`.

---

### Task 0: Branches and local patch

- [ ] **Step 1: Ask the user.** Commit the spec and plan on `feat/youtube-video` with `git commit -m "docs: design and plan watching youtube video"`. Ask in the same message whether local commits on both branches may go ahead without asking each time.
- [ ] **Step 2: Branch the fork.** Run `git -C ../ytmusic-rs checkout -b feat/video feat/web`.
- [ ] **Step 3: Add the local patch.** Append to the root `Cargo.toml` (uncommitted until Task 8):

```toml
[patch."https://github.com/jorranMLGN/ytmusic-rs"]
ytmusic = { path = "../ytmusic-rs" }
```

- [ ] **Step 4: Verify.** Run `cargo check --workspace`. Expected: `Finished`, and `cargo tree -p music -i ytmusic` shows the path source.

---

### Task 1 (fork): open and renew a video stream; music-video type

**Files:**
- Modify: `src/stream.rs` (`AudioStream`, `Renewal`), `src/player.rs`
- Fixture: `tests/fixtures/web/player_music_omv.json` (a guest WEB_REMIX player response for an official music video, scrubbed with `crate::fixtures::scrub`)

**Interfaces:**
- Produces:

```rust
impl YtMusic {
    /// The tallest H.264 format no taller than `max_height`, already arriving, renewing its url by itag.
    pub async fn open_video(self: &Arc<Self>, video_id: &str, max_height: u32) -> Result<(VideoFormat, AudioStream)>;
    pub(crate) async fn renewed_video(&self, video_id: &str, itag: u32) -> Result<VideoFormat>;
    /// `videoDetails.musicVideoType` from a Music-client player response, e.g. "MUSIC_VIDEO_TYPE_OMV".
    pub async fn music_video_type(&self, video_id: &str) -> Result<Option<String>>;
}
pub fn is_music_video(kind: &str) -> bool; // OMV and UGC true, ATV and anything else false
```

- `AudioStream` keeps its name. It gains a private constructor from a url, an agent and a total. `Renewal` gains a `kind: Kind { Audio, Video }`, so `renew` calls `renewed_audio` or `renewed_video`.

- [ ] **Step 1: Write the failing tests:**
  - `open_video_picks_the_tallest_format_under_the_cap`: a pure helper `pick(&[VideoFormat], 480) -> Option<&VideoFormat>` over the formats parsed from `player_web.json` returns `height == 480`. With a cap of 100 it returns the lowest format.
  - `a_refused_video_stream_renews_by_its_itag`: a pure helper `renewal_format(&[VideoFormat], itag)` finds the same itag and returns `None` when that itag is absent.
  - `music_video_types_classify`: `is_music_video("MUSIC_VIDEO_TYPE_OMV")` and `("…UGC")` are true, and `("…ATV")` and `("")` are false.
  - `the_music_video_type_is_read_from_a_player_response` reads the type from `player_music_omv.json`.
- [ ] **Step 2: Run** `cargo test -p ytmusic`. Expected: FAIL.
- [ ] **Step 3: Implement.** The audio path must behave exactly as before. Record the fixture with an `#[ignore]` guest recorder, read-only, then scrub it.
- [ ] **Step 4: Run** `cargo test -p ytmusic` and `cargo clippy --all-targets`. Expected: PASS and clean.
- [ ] **Step 5: Commit (fork):** `feat(player): open and renew an h264 video stream`

---

### Task 2 (video): a streaming `Movie` player

**Files:**
- Create: `crates/video/src/movie.rs`, `crates/video/src/fmp4.rs`, `crates/video/tests/fixtures/clip-dash.mp4`
- Modify: `crates/video/src/lib.rs` (`pub use movie::{Clock, Movie};`), `crates/music/src/video.rs` (re-export `Clock`, `Movie`)

**Interfaces:**
- Produces:

```rust
/// Where playback is, read by the decoder thread. `None` while paused or not yet playing.
pub trait Clock: Send + Sync + 'static { fn now(&self) -> Option<Duration>; }
pub struct Movie { /* decoder thread handle, latest frame */ }
impl Movie {
    pub fn start(source: impl Read + Seek + Send + 'static, clock: Arc<dyn Clock>) -> Result<Self>;
    pub fn frame(&self) -> Option<Arc<Frame>>;   // same Frame as Player
    pub fn watch(&self, width: u32);             // same contract as Player::watch
    pub fn pts(&self) -> Option<Duration>;       // presentation time of the latest frame
}
```

Behaviour:
- **Demux.** `fmp4.rs` reads the init segment (ftyp, moov with avc1/avcC) and the `sidx` once. It then reads one fragment at a time (moof: tfdt, trun with sample sizes and composition offsets; mdat), with no full-file read. The frame clock is PTS = (tfdt + decode offset + composition offset) / timescale.
- **Pacing.** The decoder thread decodes ahead of `clock.now()` by at most `AHEAD = 1 s` of frames. It publishes the latest frame whose PTS ≤ now.
- **Seeking.** When `now` jumps outside `[pts − 0.5 s, pts + AHEAD]`, it seeks: it finds the fragment that holds `now` through `sidx`, feeds its keyframe, decodes and drops frames up to `now`, then publishes.
- **Pause.** `None`, or a clock that does not move, makes the thread sleep on a condvar or a short timer and decode nothing.
- **Reuse.** Use `convert::bgra` and `annexb::Mp4BitstreamConverter`. The thread is named `"video"`. Dropping the `Movie` stops the thread.

- [ ] **Step 1: Generate the fixture.** Create a 4 s, 25 fps, 144p fragmented H.264 clip with a keyframe every 1 s and a single `sidx`, using `ffmpeg -f lavfi -i testsrc=size=256x144:rate=25 -t 4 -c:v libx264 -profile:v high -g 25 -movflags +frag_keyframe+empty_moov+default_base_moof+global_sidx -f mp4 clip-dash.mp4`. Keep it under 100 KB.
- [ ] **Step 2: Write the failing tests** in `movie.rs` and `fmp4.rs`. Each skips without OpenH264, except the `fmp4` ones, which need no codec. A `FakeClock(Mutex<Option<Duration>>)` drives them.
  - `the_fixture_indexes_four_fragments` (fmp4): 4 sidx entries, and the first fragment starts at pts 0.
  - `frames_follow_the_clock`: with the clock at 2.0 s, within 500 ms `pts()` is in `[1.96 s, 2.0 s]`.
  - `a_seek_lands_on_the_keyframe_before_the_target`: with the clock at 0.2 s and then at 3.3 s, within 500 ms `pts()` is in `[3.26 s, 3.3 s]`.
  - `a_paused_clock_holds_the_frame`: with the clock at 1.0 s and then `None`, `frame().index` is unchanged over 300 ms.
  - `watch_scales_the_frame_width`: `watch(128)` gives `frame().width == 128`.
- [ ] **Step 3: Run** `cargo test -p video`. Expected: FAIL.
- [ ] **Step 4: Implement.**
- [ ] **Step 5: Run** `cargo test -p video` and `cargo clippy --workspace --all-targets`. Expected: PASS and clean.
- [ ] **Step 6: Commit:** `feat(video): play a streamed video against an outside clock`

---

### Task 3 (music): `MusicApi::video`

**Files:**
- Modify: `crates/music/src/lib.rs` (beside `canvas`, `:170`), `crates/music/src/tagged/mod.rs` (beside `:108`), `crates/music/src/youtube_video/client.rs`, `crates/music/src/youtube/client.rs`

**Interfaces:**
- Consumes: Task 1 `open_video` and `music_video_type`/`is_music_video`; `music::stream::Stream::pulling_ranged`, `Ranged for AudioStream` (project A).
- Produces:

```rust
/// A video to show beside a track's sound: a seekable reader over its bytes and its size.
pub struct VideoFeed { pub width: u32, pub height: u32, pub reader: stream::Reader, pub stream: stream::Stream }
/// The video of this track no taller than `max_height`, or `None` when it has none.
async fn video(&self, _track_id: &str, _max_height: u32) -> Result<Option<VideoFeed>> { Ok(None) }
```

- `youtube_video`: always `open_video`. `youtube` (Music): only when `is_music_video(music_video_type(id))`, with the type cached per id in the client (a `Mutex<HashMap<String, bool>>`).
- `tagged` forwards with `tag::untag`.

- [ ] **Step 1: Write the failing test** `an_art_track_has_no_video` in `youtube/client.rs`. It covers a pure helper `fn shows_video(kind: Option<&str>) -> bool`: `None` and ATV give false, OMV and UGC give true.
- [ ] **Step 2: Run** `cargo test -p music youtube::`. Expected: FAIL. Then implement. Expected: PASS.
- [ ] **Step 3: Add the live test** `live_a_set_opens_a_480p_video_feed` (`#[ignore]`, guest, read-only, in `live_tests/youtube_video.rs`): `video("c0-hvjV2A5Y", 480)` returns `height == 480`, and reading 64 KiB from its reader succeeds within 5 s. Run it by exact name.
- [ ] **Step 4: Run** `cargo test --workspace` and clippy. Expected: PASS and clean.
- [ ] **Step 5: Commit:** `feat(music): hand out a youtube video feed beside the sound`

---

### Task 4 (state): `state::Video`

**Files:**
- Create: `crates/state/src/video.rs`
- Modify: `crates/state/src/lib.rs` (`mod video; pub use video::{Video, Place};`, a `pub video: Entity<Video>` field on `Sonora` beside `canvas` `:183`, constructed beside `Canvas::new` `:276`), `crates/state/src/settings.rs`

**Interfaces:**
- Consumes: Task 2 `Movie`, `Clock`; Task 3 `MusicApi::video`, `VideoFeed`; `Playback::{live_position, state, track}`, `PlaybackEvent::{Seeked, Paused}`, `Codec::ready`.
- Produces:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)] pub enum Place { #[default] None, Slot, Fullscreen, Pip }
impl Video {
    pub fn new(session: Entity<Session>, playback: Entity<Playback>, settings: Entity<AppSettings>, codec: Entity<Codec>, io: Io, cx: &mut Context<Self>) -> Self;
    pub fn movie(&self) -> Option<&music::video::Movie>;
    pub fn generation(&self) -> u64;
    pub fn has_video(&self) -> bool;      // current track has a feed (or is loading one)
    pub fn place(&self) -> Place;
    pub fn set_place(&mut self, place: Place, cx: &mut Context<Self>);
}
// settings.rs, in Appearance beside `canvas` (:372):
pub fn video(&self) -> bool;                 // default true
pub fn set_video(&mut self, on: bool, cx: &mut Context<Self>);
pub fn video_max_height(&self) -> u32;       // default 1080, one of 360, 480, 720, 1080
pub fn set_video_max_height(&mut self, height: u32, cx: &mut Context<Self>);
// in StateValues beside mini_corner (:769):
pub fn pip_corner(&self) -> MiniCorner;      // default BottomRight
pub fn set_pip_corner(&mut self, corner: MiniCorner, cx: &mut Context<Self>);
pub fn pip_size(&self) -> Option<(f32, f32)>;
pub fn set_pip_size(&mut self, size: (f32, f32), cx: &mut Context<Self>);
```

Behaviour:
- **Pure helpers**, tested directly:
  - `fn height(place: Place, cap: u32) -> Option<u32>`: `None` for `Place::None`, otherwise `min(480 or 1080, cap)`.
  - `fn keep(place: Place, idle: Duration) -> bool`: `true` unless `place == None && idle >= GRACE (5 s)`.
- **Loading.** A movie is loaded only when `settings.video() && codec.ready() && height(place) is Some`. The feed is fetched in `Io::spawn` and `Movie::start` runs in `spawn_blocking`. A height change reloads at the new height.
- **Track change.** A new track bumps `generation`, drops the movie and forgets the feed.
- **Clock.** `PlaybackClock(Arc<Mutex<Snapshot { at: Duration, since: Instant, playing: bool }>>)` gives `now() = playing.then(|| at + since.elapsed())`. It is updated from `live_position()` on every Playback notify and on `Seeked`/`Paused`.
- **Sign-out.** `SessionEvent::SignedOut` clears the state.

- [ ] **Step 1: Write the failing tests:**
  - `height_follows_the_place_and_the_cap`: Slot gives 480, Pip gives 480, Fullscreen gives 1080, Fullscreen with a 720 cap gives 720, Slot with a 360 cap gives 360, and None gives `None`.
  - `video_stops_after_the_grace_period`: `keep(None, 4.9 s)` is true, `keep(None, 5 s)` is false, and `keep(Slot, 1 h)` is true.
  - `a_seek_resets_the_clock`: a snapshot updated to 3600 s reads between 3600 and 3600.1 s.
  - `a_paused_clock_reads_none`.
  - `no_codec_means_no_video`: the pure `wanted(video_on, codec_ready, place)` returns false without the codec.
  - `a_new_track_drops_the_old_movie`: the pure `fn next(current: Option<&str>, playing: Option<&str>) -> Change { Keep, Drop }` returns Drop when the ids differ.
- [ ] **Step 2: Run** `cargo test -p state video::`. Expected: FAIL. Then implement. Expected: PASS.
- [ ] **Step 3: Run** `cargo test --workspace` and clippy. Expected: PASS and clean.
- [ ] **Step 4: Commit:** `feat(state): decide where a video plays and keep it in step`

---

### Task 5 (views): slot, fullscreen and player-bar toggle

**Files:**
- Modify: `crates/views/src/shared/canvas.rs` (generalise `CanvasArt`), `crates/views/src/chrome/aside.rs` (`QueueArt` `:264`, slot `:1854-1938`), `crates/views/src/shells/fullscreen.rs` (artwork `:393-445`, panel `:85`/`:185`), `crates/views/src/chrome/player_bar.rs` (`:242-251`, `:499-529`), `crates/input/src/lib.rs` (actions `TogglePip`, `ToggleVideo`, without default keys), `assets/icons/lucide/{video,video-off}.svg`, `assets/i18n/en-US/main.ftl`

**Interfaces:**
- Consumes: Task 4 `Video`, `Place`, `video()`/`set_video`.
- Produces:
  - `CanvasArt::video(cx) -> Self`, a second constructor reading `Sonora::global(cx).video`. Its fit is `ObjectFit::Contain`, and it uses no dimming.
  - `pub(crate) fn video_showing(cx: &App) -> bool`. Unlike `canvas_playing`, it ignores `ui::motion::animates` and `effects()`, because reduce motion does not turn video off.
  - Actions `TogglePip` and `ToggleVideo`, handled in Task 6.

Behaviour:
- **Slot.** `QueueArt::Video` is the first choice when `video_showing`, the slot is on the Queue tab, and the place is not Pip. The slot height is `width * 9 / 16` (capped by `CANVAS_SHARE`), with no `CANVAS_DIM`. Two ghost icon buttons sit on it: `picture-in-picture.svg` dispatches `TogglePip`, and `maximize.svg` dispatches `ToggleFullscreen`.
- **Reporting the place.** The aside reports `Place::Slot` or `Place::None` via `Video::set_place` when its `shown` state changes, through an observer or an effect, never from inside `render`.
- **Fullscreen.** While a video shows, `FullscreenView` sets `Place::Fullscreen` on open and `Place::Slot` on close. It hides the side panel (`panel = None`) while the video shows, without saving that to `set_fullscreen_tab`. The artwork box fills the view with `CanvasArt::video`.
- **Player bar.** A `ToggleVideo` button (`video.svg`, or `video-off.svg` when off) shows only when `Video::has_video()`. Tooltip keys `player-video-on` and `player-video-off`. Keys: `player-video-on = Show video`, `player-video-off = Hide video`, `video-pip = Picture in picture`, `video-large = Fullscreen`.

- [ ] **Step 1: Write the failing test** `the_slot_picks_video_over_canvas_and_cover` (in aside.rs). It extends `QueueArt::pick` with a `video: bool` argument: video and canvas give `Video`; not shown gives `None`; video with no cover gives `Video`.
- [ ] **Step 2: Run** `cargo test -p views aside::`. Expected: FAIL. Then implement everything above. Expected: PASS.
- [ ] **Step 3: Run** `cargo test --workspace` and clippy. Expected: PASS and clean.
- [ ] **Step 4: Commit:** `feat(views): show a playing video in the sidebar and fullscreen`

---

### Task 6 (sonora): PiP window and actions

**Files:**
- Create: `crates/sonora/src/pip.rs`, `crates/views/src/shells/pip.rs` (`PipView`)
- Modify: `crates/sonora/src/actions.rs` (`TogglePip`, `ToggleVideo` beside `:32`), `crates/sonora/src/main.rs` (`pip::watch(cx)` beside `mini::watch` `:177`), `crates/views/src/shells/mod.rs`

**Interfaces:**
- Consumes: Task 4 `Place`, `pip_corner`, `pip_size`/`set_pip_size`; Task 5 actions; `mini::{kind, placement}` and `above::raise` (make the helpers they need `pub(crate)` rather than copy them).
- Produces: `pip::{open, close, toggle, is_open, watch}`, with the same shapes as `mini.rs` `:66-113`.

Behaviour:
- **Window.** `PIP_SIZE = 480×270` unless `pip_size()` holds a size. `is_resizable: true`, `kind(true)`, and `above::raise` runs after opening.
- **Size.** When the window resizes, the size is stored with `set_pip_size`.
- **Content.** `PipView` renders `CanvasArt::video` filling the window, plus one back button (`video-back = Back to Sonora`), which closes the PiP.
- **Place changes.** Opening sets `Place::Pip`. Closing sets `Place::Slot` and leaves the aside to correct it to `None` if the slot is hidden. `ToggleFullscreen` while the PiP is open closes the PiP before going fullscreen.
- **ToggleVideo** flips `set_video`.

- [ ] **Step 1: Write the failing test** `pip_placement_uses_the_stored_size` (pip.rs). The pure `fn bounds(corner, stored: Option<(f32, f32)>, screen: Bounds<Pixels>) -> Bounds<Pixels>` gives 480×270 at bottom-right without a stored size, and the stored size at top-left with one.
- [ ] **Step 2: Run** `cargo test -p sonora pip::`. Expected: FAIL. Then implement. Expected: PASS.
- [ ] **Step 3: Run** `cargo test --workspace` and clippy. Expected: PASS and clean.
- [ ] **Step 4: Commit:** `feat(sonora): float a playing video in its own window`

---

### Task 7: settings rows, copy, changelog, docs, app run

**Files:**
- Modify: `crates/views/src/screens/settings.rs` (slots beside `Slot::Canvas` `:171`, rows beside `canvas_row` `:2039`), `assets/i18n/en-US/main.ftl`, `CHANGELOG.md`, `docs/architecture.md`

Rows and keys:
- `settings-video = Video`, with `settings-video-detail = Shows the video of a YouTube track beside its sound`.
- `settings-video-quality = Maximum video quality`, a select of 360p, 480p, 720p and 1080p.
- `settings-pip-corner = Picture-in-picture corner`, reusing the existing `mini-corner-*` ids.
- The existing codec row also shows when the video setting is on.

- [ ] **Step 1: Add the rows and keys.**
- [ ] **Step 2: Add the CHANGELOG entry** under `Added`: "Watch YouTube videos and music videos while they play: in the sidebar, full screen, or in a floating window that stays on top."
- [ ] **Step 3: Add `docs/architecture.md` notes:** the `Movie` player and its `Clock`, `state::Video` places and the grace period, and the PiP window.
- [ ] **Step 4: Run** `cargo fmt`, `cargo test --workspace` and `cargo clippy --workspace --all-targets`. Expected: PASS and clean.
- [ ] **Step 5: App run (controller with the user).** `cargo run --locked -p sonora`. Play a Boiler Room set and check, in order:
  - the video shows in the slot;
  - a seek to an hour in brings the picture there too;
  - Fullscreen gives a large video with no panel;
  - PiP floats above other windows, and its back button returns it;
  - closing the sidebar makes the video log stop within 5 s while the sound continues;
  - a YouTube Music music video shows video, and an album track shows its cover;
  - toggling the video off leaves sound only, and the setting survives a restart.
- [ ] **Step 6: Commit:** `feat(settings): choose video, its quality and the pip corner`

---

### Task 8: pin the fork

- [ ] **Step 1: Ask the user** before `git -C ../ytmusic-rs push -u origin feat/video` (remote `jorranMLGN/ytmusic-rs`, new branch).
- [ ] **Step 2: Repoint and verify.** Set `ytmusic` to `rev = "<pushed head>"` on `jorranMLGN/ytmusic-rs` and remove the `[patch]`. Run `cargo update -p ytmusic` and `cargo check --workspace --locked`. Expected: `Finished`.
- [ ] **Step 3: Commit:** `build(music): take ytmusic from the fork that opens video`
