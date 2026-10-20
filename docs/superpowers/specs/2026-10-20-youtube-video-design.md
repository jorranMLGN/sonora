# Watching YouTube video — Design

When what is playing is a video, Sonora shows it. By default it plays in the cover slot at the
top of the right sidebar. It can go large in the app's fullscreen view, or out into a floating
picture-in-picture window that stays above other applications. A button turns the video off and
leaves the sound. This is project B. Project A (`2026-10-19-youtube-provider-design.md`) built
the plain-YouTube provider and prepared H.264 formats for it.

## Why

DJ sets and live recordings are made to be watched as much as heard. Leaving Sonora for a
browser tab to see the set loses the queue, the scrobbles and the player. The listener wants to
glance at it while browsing, watch it full size, or keep it in a corner over other work.

## Goals

1. **Default view**: a playing video is shown in the cover slot above the queue in the right
   sidebar (`chrome/aside.rs`, where a Canvas or the cover sits now).
2. **Large**: the app's fullscreen view (`shells/fullscreen.rs`) shows the video across the
   whole area, with the transport over it while the pointer moves.
3. **Picture-in-picture**: a separate floating window, draggable and resizable, kept above other
   applications. A button sends it back into the app.
4. **Off switch**: a player-bar button turns video off. Off means sound only, and no video is
   downloaded or decoded.
5. **In sync**: the picture follows the audio, through pauses and seeks.
6. **Which tracks**: every track of the YouTube provider (`youtube-video`), and YouTube Music
   tracks that are themselves videos (official music videos and user uploads), never an art
   track's still picture.

## Non-goals

- Other providers. Spotify Canvas stays as it is.
- Subtitles, chapters, comments, live chat.
- VP9, AV1, and anything above 1080p. YouTube has no H.264 above 1080p.
- Switching a YouTube Music album track to its separate music-video version.
- Mobile (`crates/mobile`, `apps/ios`).

## Anti-goals

- Picture and sound drifting apart, or a seek that leaves the picture behind.
- Windows opening on their own: closing the sidebar never pops up a PiP.
- Spending bandwidth or CPU on video nobody can see.
- Shipping or bundling OpenH264 in any form.

## Decisions (from the conversation)

1. **Where it shows, in order of the listener's choice**: sidebar slot (default), fullscreen in
   the app, floating PiP. The slot carries two buttons on the video: large and PiP. The player
   bar carries the video on/off toggle and stays the way to reach large or PiP when the sidebar
   is closed.
2. **No visible place → no video work.** With the sidebar closed, the window too narrow for it,
   or the mini player in use, and neither fullscreen nor PiP open, the video stops downloading
   and decoding while the sound plays on. When a place comes back, the picture resumes at the
   current position. A PiP or fullscreen that is already open keeps playing regardless of the
   sidebar. No PiP opens by itself.
3. **Quality follows size, automatically**: 360p–480p for the sidebar slot and the PiP, up to
   1080p in fullscreen, chosen from the H.264 formats the provider offers. A setting "maximum
   video quality" caps it.
4. **PiP window** follows the existing mini-player window (`crates/sonora/src/mini.rs`,
   `crates/sonora/src/above.rs`): `WindowKind::Floating` on macOS, `PopUp` on Windows, an
   ordinary window raised with `_NET_WM_STATE_ABOVE` on X11. On Wayland it is an ordinary window,
   since Wayland has no always-on-top. (The user runs X11.)
5. **OpenH264** is fetched from Cisco on request through the existing consent flow
   (`state::Codec`), as for Canvas. Without it, video stays off and the cover shows.
6. **YouTube Music**: a track shows video only when it is a music video or upload
   (`musicVideoType` OMV or UGC), never for an art track (ATV), whose video stream is a still
   image. How that is learned: decision 15.

7. **Large is fullscreen, with the video filling it.** Opening fullscreen while a video plays (the
   slot's large button, the `f` key or the player-bar button) shows the video across the whole
   view; the lyrics/queue panel is hidden and comes back through its existing toggle. Leaving
   fullscreen returns the video to the slot. (Decided by the user.)
8. **The sidebar slot turns 16:9 while a video plays**, letterboxed if needed, without the 0.8
   dimming the cover and Canvas get (`aside.rs:41`, `:1872-1874`, `:1938`). Canvas and covers keep
   the square, dimmed slot. (Decided by the user.)

## Decisions (my calls)

9. **Reduce motion does not turn video off.** Video is content the listener asked for with its
   own switch, not decoration like Canvas.
10. **The video toggle is remembered** across launches in settings (default on), next to the
   Canvas setting (`AppSettings::canvas`, `crates/state/src/settings.rs:1340`).
11. **The PiP window** remembers its size; its corner is a setting like the mini player's
   (`MiniCorner`, `crates/sonora/src/mini.rs:39-63`), since gpui can size a window but not move
   one (`mini.rs:104`). It is resizable and always on top (no separate on-top setting).
12. **One place at a time.** The video plays in exactly one of slot, fullscreen and PiP, with one
   decoder at one quality. While the PiP is open the slot shows the cover. Opening fullscreen
   while the PiP is open moves the video into fullscreen and closes the PiP.
13. **Another sidebar tab counts as no place.** The slot exists only on the Queue tab
   (`aside.rs:1854`). On Lyrics, Jam or Similar the video stops after a 5 s grace period, so
   flicking between tabs does not restart the download each time.
14. **The video byte source lives in the ytmusic fork**, beside the audio one: an opener for a
   `VideoFormat` with the same ranged reads and url renewal that audio has (`AudioStream::open`
   takes an `AudioFormat`, `src/stream.rs:57`; renewal is audio-only, `src/player.rs:139`,
   `:193-211`). Without renewal a cold guest url is refused after about a megabyte
   (`src/stream.rs:6-8`). Sonora's `Cargo.toml` moves to the new fork rev; pushing the fork is
   asked for separately.
15. **YouTube Music's music-video check** is one Music-client player request per track
   (`YtMusic::player_response`, `src/player.rs:52`), reading `musicVideoType`, made only when the
   video toggle is on and a place is visible, and cached per track id. The shared `Track` model
   gets no new field. The guest VisionOs player response is not relied on for it.

## Constraints

- `crates/video` stays a leaf that knows nothing about music or gpui. `music::video` is the only
  door `state` and `views` use. `openh264` stays `libloading`-only.
- `state` and `views` reach a provider only through `MusicApi`; only `main.rs` names a provider.
- Network work runs on tokio (`Io::global(cx).spawn`); a view never drives a player — playback
  state goes through `state::Playback`.
- Theme tokens, `ui::Room` breakpoints and Fluent strings as in CLAUDE.md.

## What exists (verified 2026-10-20)

- **`crates/video`**: `codec.rs` (OpenH264 store, `api()`, `install()`), `convert.rs` (YUV → BGRA
  with nearest-neighbour downscale), `annexb.rs`, and `player.rs`, a looping player for short
  silent clips: whole file in memory via `mp4` + `Cursor`, all packets parsed up front, its own
  wall clock, no seek, no timestamps, a 60 s download cap.
- **`state::Canvas`** (`crates/state/src/canvas.rs`) loads the provider's `canvas(id)`, runs the
  player in `spawn_blocking`, follows `Playback` only to pause and resume. **`state::Codec`** holds
  the consent states.
- **`views::shared::canvas::CanvasArt`** pulls the latest frame each display frame
  (`window.on_next_frame`), uploads it as a `RenderImage`, drops the old one, and stops 500 ms
  after its last render. It is mounted in the sidebar slot and the fullscreen artwork box
  (`fullscreen.rs:400`, `"fullscreen-artwork"`).
- **`state::Playback::live_position()`** (`playback.rs:2028`) runs on wall time between engine
  reports and eases towards them; `Seeked` and `Paused` events are the resync points.
- **`music::stream::Stream::pulling_ranged`** (project A) downloads with holes and jumps where a
  reader reads, for any `Ranged` source. `ytmusic::AudioStream` is that source for an audio
  format only (see decision 14).
- **`YtMusic::video_formats(video_id)`** (fork, project A) returns H.264 `VideoFormat`s with
  width, height, fps, bitrate, url, `init_range` and `index_range`, sorted by height.
- **OpenH264 spike (2026-10-19)**: Main and High profile from YouTube decode without errors,
  frames equal ffmpeg's, 114–364 fps on one thread at 1080p–720p. Playback must be by
  presentation timestamp, not frame count (an edit list shifted one sample by 11 frames).

## What the builder decides

- The `MusicApi` seam for a video source (e.g. `async fn video(&self, id) -> Result<Option<…>>`
  beside `canvas`) and what it hands back (formats plus a way to open a ranged byte source), so
  `crates/video` gets bytes without knowing about providers.
- How `crates/video` gains a streaming player next to the looping one: fMP4 demux by fragment
  (the `sidx` index gives the byte range of each segment, as `youtube/segments.rs` does for
  audio), decode by PTS against an external clock, seek to the keyframe at or before a target,
  a bounded buffer ahead of the playhead.
- How the player is told the clock: a closure or trait reading `Playback::live_position()`, with
  resync on `Seeked` and `Paused`.
- When to switch format as the view grows or shrinks (e.g. on entering fullscreen), and whether
  to keep the old one until the new one has a frame.
- Frame upload cost at 1080p: whether `CanvasArt` is reused as is, or gets a cheaper path.
- The exact control layout on the video and in the fullscreen view, within theme and room rules.

## Done when

- Playing a Boiler Room set from the YouTube provider shows the video in the sidebar slot within a
  few seconds, in sync with the sound; seeking to an hour in brings the picture there too.
- The large button shows it across the fullscreen view at up to 1080p; the PiP button moves it to
  a floating window above other applications on X11; its back button returns it.
- Closing the sidebar stops video downloading and decoding (visible in the log) while sound plays
  on; reopening shows the picture at the current position.
- A YouTube Music music video shows video; a YouTube Music album track shows its cover.
- The video toggle off leaves sound only; the setting survives a restart.
- `cargo clippy --workspace --all-targets` stays clean; `CHANGELOG.md` has an `Added` entry.
