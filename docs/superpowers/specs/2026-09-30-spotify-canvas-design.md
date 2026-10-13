# Spotify Canvas — Design

Spotify lets an artist attach a *Canvas* to a song: a short (3–8 s), silent, vertical H.264 video that
loops in place of the cover. Sonora plays it in two places: the fullscreen view and a new now-playing
block at the top of the right sidebar's Queue tab.

## What was verified (2026-09-30)

- `collection::extended(session, uris, ExtensionKind::CANVAZ)` answers per track with a
  `librespot_protocol::canvaz::entity_canvaz_response::Canvaz`. Two of four probed tracks carried one
  (`type = VIDEO_LOOPING_RANDOM`, `url = https://canvaz.scdn.co/upload/artist/<id>/video/<hex>.cnvs.mp4`);
  the other two answered an empty extension (0 bytes), which means *no canvas*.
- The CDN answers without a token (`206 Partial Content` on a range request).
- `ffprobe`: H.264 **Main** profile, `yuv420p`, `color_range=tv`, `color_space=bt709`, 718–720 × 1280,
  25–30 fps, 7–8 s, 200–210 frames, no audio track.
- `openh264` 0.9.8 decodes both files with zero errors in 4.1–5.8 ms a frame (Core Ultra 7 155H).
  Its `write_rgba8` uses the BT.601 matrix and falls to a scalar path on odd widths (15 ms a frame at
  718 wide), so Sonora converts YUV itself.

## Decisions

1. **No Web API.** Data comes from `spclient` extended metadata, the same batched route `TRACK_V4`
   uses. No hand-rolled protobuf: the generated `Canvaz` type exists.
2. **The decoder is never shipped.** `openh264` is built with `default-features = false,
   features = ["libloading"]`. Cisco's own `libopenh264` 2.6.0, the one binary Cisco's H.264 patent
   licence covers, is downloaded from `https://ciscobinary.openh264.org/<name>.bz2` with the user's
   consent, the way the Widevine CDM is. `OpenH264API::from_blob_path` checks its sha256 against the
   crate's own list before loading it. No release artefact, package or Flatpak carries it, and
   `about.toml` gains no entry for it.
3. **A new leaf crate, `video`**, knows nothing about music or gpui: the decoder store, the download,
   the MP4 → Annex-B → YUV → BGRA pipeline and a looping frame player on its own thread.
   `music::video` is the only door `state` uses, as `music::drm` is for Widevine.
4. **Sonora converts YUV itself**: BT.709, limited range, downscaled in the same pass to the width the
   screen asks for, written as **BGRA** because GPUI's `RenderImage` takes BGRA (`ui::artwork::bgra`).
5. **Motion rules.** A canvas plays only while `ui::motion::animates(cx)`, `shared::effects()` and the
   new `appearance.canvas` setting all allow it, while playback is `Playing`, and while a surface is
   actually drawing it. Otherwise the cover stays and the decoder thread sleeps.
6. **Consent flow.** The first time a song with a canvas plays and no decoder is found, one modal
   asks: Download / Not now. "Not now" lasts for the run; Settings keeps a row to download or remove
   the decoder, carrying Cisco's attribution line.
7. **Right sidebar.** The sidebar has no large cover today (only the small queue row). A canvas block
   appears at the top of the Queue tab *only while a canvas plays*; with none, the tab is unchanged.

## Out of scope

- `VIDEO_LOOPING_RANDOM`'s random start point (always starts at frame 0).
- Canvas on the player bar, the mini player or the jam page.
- Hardware decoding.
- Any provider but Spotify: every other `MusicApi` answers `Ok(None)`.
