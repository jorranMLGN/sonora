# A jam on the local network: sending the audio

Status: design proposed, not approved
Branch: `feat/multi-provider`
Audience: this fork only. Not intended for upstream.

Supersedes the synchronised-session design that carried this filename. That
version shared ids and let each device fetch its own audio. The user chose the
other model: **Sonora sends the decoded audio itself.**

## What this changes

Sonora opens a **jam**: a listening room on the local network. Anything Sonora
plays — Spotify, SoundCloud, YouTube Music, a file on disk — is tapped after
decoding and sent as PCM to whoever joined.

Two kinds of receiver, both in v1:

- **A browser.** A phone, tablet or laptop opens a page Sonora serves, taps
  play, and hears the room. Nothing installed, no account, no provider.
- **A second Sonora.** It registers as a receiver, plays through cpal, and
  corrects its own rate against a shared clock.

## The naming, so nobody renames it

The feature is **Jam** to the user (`jam-*` in Fluent, "Jam" in the UI) because
that is the word the user asked for. In the code the audio half is
`music::cast` and the network half is `state::jam`, because "cast" is what the
tap does and "jam" is what the session is. Keep both.

## What it costs to send decoded audio

librespot decodes Spotify audio for local playback. This design sends those
samples to other machines, and the same for SoundCloud and YouTube Music. That
is a different act from playing them locally, and the providers' terms address
it. The user has been told and is going ahead deliberately.

`music::local` does not raise the question: those are the user's own files.

Nothing further is offered here. It is a fact on the record, not a caveat to
repeat.

## Where the tap goes

There is exactly one place all four providers share, and the recon measured it:

```
crates/music/src/audio/mod.rs:195-198   let output = sample * self.current;
                                        if let Some(tap) = self.tap.as_mut() { tap.push(output); }
```

`Output::open` is called from `spotify/sink.rs:46`, `soundcloud/playback.rs:221`,
`youtube/playback.rs:182` and `local/playback.rs:146`. Every other candidate is
provider-specific. `BlazingSink::write` has a tidy `&[f32]` at a fixed 44100/2,
but only Spotify has a `BlazingSink`.

Three changes to that line, each for its own reason.

### 1. Uniform the source first, so the stream has one format

Today `Mixer::add` wraps the `SmoothGain` in a `UniformSourceIterator`
(rodio `src/mixer.rs:57-65`), so the tap sits **inside** the wrapper and sees
source rate, which changes per track and — at a gapless segue — mid-stream with
no event (`SmoothGain::resync`, `audio/mod.rs:155-166`).

Pull the resampling forward in `Output::open`:

```rust
let uniform = UniformSourceIterator::new(source, config.channel_count, config.sample_rate);
stream.mixer().add(SmoothGain::new(uniform, volume.clone(), applied, RAMP)
    .with_tap(tap)
    .with_cast(cast));
```

`UniformSourceIterator` is public (`rodio::source::UniformSourceIterator`), so
this needs no fork. `Mixer::add` then wraps it a second time with identical
parameters.

Two things fall out for free:

- The cast stream has **one rate and one channel count for the life of the
  `Output`**. A track change and a gapless segue become invisible to the
  network, which is exactly right — they are invisible to the speakers too.
- `Spectrum::attach` is handed the *device* rate (`audio/mod.rs:79`) while the
  tap currently delivers *source* rate. That mismatch only skews `band_edges`
  today, so nobody noticed. After this change the two agree. A latent bug fixed
  as a side effect, not the goal.

**Not verified:** that the second, identity-parameter `UniformSourceIterator`
passes through unchanged. The parameters say it should; `src/source/uniform.rs`
was not read and nothing was measured. The plan listens for it.

### 2. Send the unscaled sample

`sample` is available on the same line, before the volume multiply. The host's
volume knob belongs to the host's speakers; a receiver has its own. Spectrum
keeps `output`, because the visualiser should show what the host hears.

So `SmoothGain` carries two consumers with different inputs — not a `Vec<Tap>`,
because they are not the same signal:

```rust
tap:  Option<Tap>,   // post-gain, what this machine hears
cast: Option<Cast>,  // pre-gain, what the network gets
```

### 3. Count what is dropped

`Tap::push` is `self.producer.push(sample).ok()` (`spectrum.rs:66-68`). Silent
drop is right for an FFT and wrong for a stream: a splice is a click, and the
receiver has no way to know it happened.

`Cast::push` keeps the same realtime contract — never block, never allocate —
and adds one `AtomicU64`:

```rust
pub fn push(&mut self, sample: f32) {
    if self.producer.push(sample).is_err() {
        self.dropped.fetch_add(1, Ordering::Relaxed);
    }
}
```

The chunker reads and clears that counter. A non-zero count marks the chunk
discontinuous; receivers flush and re-anchor rather than play a splice. A drop
is a bug worth seeing, so it also logs at warn, rate-limited.

The ring is sized from the real rate at open: **one second**, 48000 × 2 = 96000
`f32` ≈ 375 KiB per `Output`. The spectrum's 8192 (≈93 ms) is far too tight for
a consumer that is a scheduled thread rather than a tight FFT loop.

## Four outputs, one jam

`state::Playback` keeps one engine per connected provider, each with its own
live `Output`, its own cpal stream and its own `Spectrum` thread
(`playback.rs:285`). Only one sounds; `silence_others` (`406-413`) pauses the
rest, and a paused `rodio::Player` yields nothing, so `SmoothGain::next` is not
pulled.

That is nearly enough to ignore the problem, but not quite: during a switch the
old engine is still ramping down (25 ms for SoundCloud and YouTube) while the
new one starts. Two producers into one `rtrb` ring is unsound — it is SPSC.

**So each `Output` owns its own ring, and the broadcaster drains one.**

How the rings reach `state`: `PlaybackFactory::start` gains a second parameter.

```rust
fn start(&self, config: PlaybackConfig, cast: Option<CastSink>)
    -> (Box<dyn Player>, Box<dyn PlaybackEvents>);
```

`CastSink` is `Arc<dyn Fn(Feed) + Send + Sync>`; `Feed { slug, rate, channels, samples: Consumer<f32>, dropped: Arc<AtomicU64> }`. `Output::open` builds the
ring, keeps the producer, and calls the sink with the consumer. `PlaybackConfig`
stays `Copy` and unchanged — it is a settings bag and a callback is not a
setting.

`state::Playback::start_engine` (`playback.rs:1420-1444`) is the one place a
config is built and `start` is called, so it is the one place to thread this
through.

`restart_output` (`1365-1386`) and `rebind` (`1398-1418`) throw an `Output`
away and open a new one. The new `Feed` replaces the old one under the same
slug, and the broadcaster emits a `Cut`. No special case needed.

**Which ring is live** is a question `Playback` already answers:
`current_slug()` (`1338-1341`). The broadcaster follows it. On a change it
switches ring and emits `Cut`, because the two rings have independent sample
counts and splicing them would drift.

## What the tap position gives for free

The tap sits **downstream** of the `rodio::Player` queue —
`Output::open` does `let (sink, source) = rodio::Player::new()` and wraps
`source`. So `sink.clear()` removes queued chunks *before* the tap ever sees
them.

That single fact settles four of the questions this design was supposed to
answer:

| Event | What the protocol has to do |
| --- | --- |
| **Seek** | Nothing. `BlazingSink`'s flush (`sink.rs:107-110`) drops the queue upstream of the tap; the tap sees only what plays. SoundCloud and YouTube mute, drain and `try_seek` — again upstream. |
| **Track change** | Nothing. Samples keep coming. |
| **Gapless segue** | Nothing. rodio slides to the next source inside the audio thread, and after change 1 the format cannot shift. |
| **Volume** | Nothing. Change 2 taps before the multiply. |

**The tap sees exactly what the speakers see.** That is the property the whole
design rests on, and it is why this model needs a far smaller protocol than the
synchronised-session one did.

Only two things still need saying out loud, and both are about the *absence* of
samples:

- **`Quiet`** — the ring has been dry for more than a chunk period. A paused
  player produces nothing, and a receiver holding 250 ms would otherwise keep
  playing after the host went silent. The mark carries the sample index where
  silence begins; a receiver plays up to it and then outputs silence, keeping
  its timeline running. Resuming needs no mark: samples simply start again.
- **`Cut`** — the source was replaced: a provider switch, a device change, a
  dropped-sample count above zero. Receivers flush and re-anchor.

**A consequence to state plainly:** the host's own speakers go quiet almost at
once on a pause, and receivers go quiet one lead-time later. That is inherent to
buffering ahead and shrinks only by shrinking the lead.

## The wire

**Raw PCM. No Opus in v1.** An encoder adds 20–60 ms of algorithmic delay, and
there is no safe Opus encoder in the tree — `opusic-sys` is in the lock only as
a transitive dependency of a *decoder* adapter, so using it means new `unsafe`
code or a new crate. Compression is the obvious later optimisation and the spec
does not design against it.

**Interleaved little-endian `i16`.** At 44100/2 that is 176 400 B/s =
**1.41 Mbit/s** per receiver; at 48000/2, 1.54 Mbit/s. Four receivers is about
6 Mbit/s — comfortable on wired or 5 GHz, marginal on a busy 2.4 GHz band.
`f32` would double it for no audible gain: the source is already lossy for
three of the four providers, and a receiver converts to `f32` on arrival anyway.

The `f32` → `i16` conversion happens on the chunker thread, never on the audio
thread. The audio thread does one ring write per sample and nothing else.

**The codec is named on the wire even though there is only one.** `Format`
carries a `codec` field and `Hello` carries an `accepts` list, so a later Opus
build negotiates instead of bumping `protocol`. v1 sends `Pcm16` and nothing
else: a host that sees no codec it can send refuses with `Refusal::Codec`, and
a receiver that is offered one it does not know closes rather than playing
noise. This costs one enum and one field now and saves a protocol break later.

**Chunks of 20 ms** — 882 frames at 44100, 3 528 bytes of payload. Fifty frames
a second per receiver.

### Messages

One WebSocket per receiver carries everything: binary frames are audio, text
frames are control.

**Binary frame** — a 16-byte header then the samples:

```
u32  magic          0x534E4A31  ("SNJ1")
u32  seq            chunk number since the session began
u64  first_sample   index of this chunk's first frame in the session
     ...            interleaved i16 le, `frames * channels` of them
```

`first_sample` rather than a timestamp: it is exact, it never drifts, and it is
what the receiver needs to know where a chunk belongs after a gap.

**Text frames**, JSON, `#[serde(tag = "kind")]`:

```rust
// receiver -> host
Hello   { protocol: u32, name: String, native: bool, code: String,
          accepts: Vec<Codec> }
Ping    { t0: u64 }
Bye

// host -> receiver
Welcome { protocol: u32, room: String, format: Format, lead_ms: u32, origin: u64 }
Refused { reason: Refusal }          // Protocol | Code | Codec | Full | Closed
Mark    { mark: MarkKind, at: u64 }  // Quiet | Cut, at = sample index
Now     { title, artist, album, cover: Option<String>, duration_ms: u64,
          provider: String }
Transport { playing: bool, position_ms: u64 }
Pong    { t0: u64, t1: u64, t2: u64 }
Ended   { reason: Farewell }         // HostLeft | Kicked

struct Format { rate: u32, channels: u16, codec: Codec }
enum Codec { Pcm16 }                 // the only one v1 speaks
```

`Now` is metadata for the page to show, at whatever rate the track changes, and
`provider` is the slug the track's id carries, so a receiver can say where the
sound comes from. `Transport` is the host's own transport state, sent whenever
it moves — roughly twice a second while playing. Neither is part of the audio
timeline: a receiver that ignores both still plays correctly, which is why they
are separate messages rather than fields on a chunk.

`position_ms` is the host's presentation position, not the sample cursor. A page
anchors on it and counts forward locally, the way `state::LiveClock` does, so the
readout is smooth between updates without pretending to be sample-accurate.

`protocol` is a single `u32`, refused on mismatch, no negotiation, and it stands
at **2**: adding `provider` and `Transport` changed the shape on the wire, and a
build that predates them should be turned away rather than half work. The codec
is the one thing that does negotiate, because it is the one thing v1 already
knows will grow.

## The clock

### There is no audio clock in this codebase

The recon is unambiguous: nothing ties a sample to a wall-clock time. librespot
reports position every 500 ms from its decoder, upstream of `BlazingSink`'s
queue and the device buffer. `rodio::Player::get_pos` has 5 ms resolution and
also sits upstream of the device. `Output::open` does not even read
`buffer_size` (`audio/mod.rs:54-60`). `LiveClock` is a presentation clock for
the scrubber.

So the protocol brings its own, and it is built from the only exact quantity
available: **the count of samples the broadcaster has drained.**

### The contract

The host is the master. Chunk `N`'s first frame is `first_sample`, and its
nominal presentation time is

```
t_play = origin + first_sample / rate + lead
```

where `origin` is the host clock at the moment the session started and `lead` is
the buffer receivers are given — **250 ms by default**, settable.

A receiver estimates the host↔receiver offset with an SNTP round trip over the
same socket (`Ping`/`Pong`, every five seconds), keeps the last eight samples
and uses the one with the **smallest** round-trip time, because on a LAN the
least-delayed probe is the least distorted:

```
offset = ((t1 - t0) + (t2 - t3)) / 2
rtt    = (t3 - t0) - (t2 - t1)
```

### What can and cannot be locked

**Receivers can be locked to each other.** They share one master and one sample
index, so their error is the sum of two clock estimates and two output
latencies.

**No receiver can be locked to the host's own speakers.** The tap is upstream of
the host's device buffer and nothing in this tree measures that buffer. The
host's own offset is therefore unknown, and if the host is itself a listening
room it needs the same manual slider a browser gets.

That is an honest limit, not a defect to be fixed later without new measurement.

### A native receiver corrects by rate, never by seek

It owns its own `Output`, fed from a jitter buffer, and measures the error
between where its playback cursor is and where `t_play` says it should be.

| Error | What it does |
| --- | --- |
| < 2 ms | nothing |
| 2 ms – 100 ms | drop or duplicate one frame, at most once per 50 ms |
| > 100 ms, or a `Cut` | flush the buffer and re-anchor at the current chunk |

One frame at 44100 Hz is 22.7 µs — inaudible. One frame per 50 ms is about
450 ppm of slew, enough to absorb 10 ms of error in roughly 22 seconds. A seek
is never used: it flushes, and a flush is a click.

**Target: within 10 ms of another native receiver** once settled. A proper
variable-ratio resampler (`rubato`) would do better and is the upgrade path; it
is a new dependency and v1 does not take it.

### A browser corrects by hand

The page schedules each chunk on the `AudioContext` timeline, so *into the
graph* it is sample-accurate. Out of the graph it is not, and the page cannot
find out: `AudioContext.outputLatency` is inconsistently implemented and does
not cover the OS mixer or a Bluetooth link, which adds 100–300 ms on its own.

So each browser receiver gets a **manual offset slider**, ±500 ms, remembered in
that device's `localStorage`. The user drags it until the room sounds right.

**Realistic target: 20–50 ms** between two browsers once both are trimmed, and
whatever the user's ear accepts against a native receiver. On Bluetooth the
slider is doing all the work.

### Why scheduled buffers and not an AudioWorklet

The brief asks for an AudioWorklet. It cannot be used here, for a reason that
is structural rather than stylistic:

**`AudioWorklet` requires a secure context.** `http://192.168.1.10:8990` is not
one — only `https://`, `http://localhost` and `http://127.0.0.1` are — so
`audioWorklet.addModule()` is simply unavailable on the page a phone opens.
Serving TLS on a LAN means a certificate no browser trusts, and a browser is
markedly harsher about an untrusted certificate on a WebSocket upgrade than on a
page load.

The alternative needs nothing special and is arguably the better fit: build an
`AudioBuffer` per chunk and schedule it with
`source.start(when)` on the context's own clock. That is push-based, which is
what a network stream is, where a worklet is pull-based and would need its own
ring inside the audio thread anyway. `ws://` from an `http://` page is allowed;
only `https://` pages block mixed-content WebSockets.

This is a correction to the brief, not a preference. If the page is ever served
over TLS or from localhost, the worklet path reopens and buys tighter control
over underrun behaviour.

**Autoplay** is handled by the same Play button: an `AudioContext` starts
`suspended` and `ctx.resume()` inside the tap's event handler is the gesture
browsers require.

## The page

**Served by Sonora, on the jam's own port**, at `http://<host>:8990/c/<code>`.
One HTML file with its CSS and JS inline — no build step, no CDN, no external
fetch, because a receiver has no guaranteed route to the internet.

**It lives in the binary as `include_str!`**, beside the server module, the way
`crates/i18n/src/language.rs` already embeds the `.ftl` files. It does not go
through `crates/sonora/src/assets.rs`: that is the GPUI `AssetSource` for the
app's own icons and fonts, and the server is in `state`, two crates away.

**Its strings come from Fluent.** The page carries `{{jam-page-play}}`-style
placeholders and the server substitutes them with `i18n::lookup` when it serves
the file — `state` already depends on `i18n`. So the page follows the *host's*
language. A receiver whose own language differs gets the host's; matching the
receiver would mean parsing `Accept-Language` and shipping every locale to the
page, which v1 does not do.

**The artwork is the album's largest, not the list thumbnail.** `Track::cover` is
deliberately the smallest image a provider offers, because it feeds grids; a page
filling a phone screen at three device pixels per CSS pixel needs the other end
of the range. `AlbumDetail::cover_max` carries it, `state::Cover` resolves and
caches it for whatever is playing, and the jam sends that URL when it has one.

The page shows: the room name, the artwork and the track from `Now`, badges for
the connection, the provider and the host's transport state, a progress bar that
counts forward locally between `Transport` messages, a Play/Stop button, the
offset slider, and a diagnostic line — buffer depth, stream format, chunks
received and the last round trip.

**The diagnostics are not decoration.** An `AudioContext` only makes sound after
a gesture, so a page that has received everything correctly and been tapped
nowhere is indistinguishable from a broken one unless it says so. The page
therefore separates what arrived from what was scheduled: a chunk counter that
rises while nothing plays points at the browser, and a counter that stays at zero
points at the host. It also renders a JavaScript failure on the page itself,
because a receiver has no console a user will open.

## Security

**A six-digit pairing code**, generated when the jam starts and shown in the UI.
It is in the URL path, so a receiver types one address and is done:
`http://192.168.1.10:8990/c/204813`. The WebSocket upgrade validates it again.

**Rate limit before anything else.** Five refusals from one address inside a
minute blocks that address for a minute, checked before the code is compared.
Six digits over TCP is otherwise enumerable in minutes.

**What a receiver may do: listen.** Nothing else. There is no transport command
in the protocol, no queue write, no library access. A receiver is a speaker. If
remote control is wanted later it is a separate design, and the synchronised
session that used to carry this filename is where it would come from.

**No credential, token or account identifier crosses the wire.** Not in
`Hello`, not in `Welcome`, not in `Now`. There is no field that could carry one,
and that is a design invariant rather than a setting.

**Plaintext, and why that is acceptable here.** The link carries audio the host
is already playing out loud in the same building, plus a track title. TLS on a
LAN needs a certificate no browser trusts, which costs a warning on every
device, blocks nothing an attacker on the LAN could not get by listening at the
door, and — see above — is the reason the worklet path is closed rather than
open. The code admits; it does not encrypt. That is the whole claim.

**LAN only by construction.** The listener binds `0.0.0.0`; there is no relay,
no rendezvous, no UPnP and no port mapping anywhere in the design.

## Discovery

**Manual `host:port` in v1.** The host shows its LAN address and the code; a
native receiver takes them in a field, a browser takes them in the URL bar.

mDNS is out of scope. It is a new dependency, a `THIRD-PARTY.md` regeneration,
and unreliable on exactly the networks this feature lives on — home APs filter
multicast and guest VLANs drop it. It buys a user four seconds of typing.

## Where the code lives

| Crate | What goes in it | Why |
| --- | --- | --- |
| `music` | `cast.rs` — `Cast`, `Feed`, `Format`, `CastSink`, and the receiver-side `Sink`. The `Output` and `SmoothGain` changes. | The tap is audio, and audio is `music`. Plain Rust, no gpui, no sockets. |
| `state` | `jam/` — the chunker thread, the broadcaster, the HTTP and WebSocket server, the receiver client, the `Jam` entity, the settings. | CLAUDE.md: "new network-backed features belong in a `state` entity". All of it runs on `Io`. |
| `views` | The panel, the player-bar button, the settings rows. | Where chrome lives. |
| `sonora` | Nothing. | |

### Visibility

`mod audio` and `mod spectrum` are private in `music` (`lib.rs:1,13`), and
`Spectrum` reaches the outside through `pub use spectrum::Spectrum`. The cast
follows that exact precedent:

```rust
mod cast;
pub use cast::{Cast, CastSink, Feed, Format, Sink};
```

`mod audio` **stays private.** What becomes public is the *contract* — what a
feed looks like and how to consume one — not `Output`, not `SmoothGain`, not
`Volume`. The receiver-side `Sink` is public because `state` has to drive it,
but it owns its `Output` internally and exposes only `push`, `drift` and
`nudge`, mirroring how `Player::spectrum()` (`lib.rs:202-204`) already hands out
a view without handing out the engine.

### The runtime split

The provider engines each run their own current-thread tokio runtime on their
own thread (`soundcloud/playback.rs:201-211` and siblings) and cannot reach
`state::Io`. Nothing in this design asks them to: the `CastSink` callback is a
plain `Fn`, called on whichever thread opens the `Output`, and all it does is
hand a consumer to an `UnboundedSender`. Everything after that is on `Io`.

The chunker is **a dedicated OS thread**, not a tokio task. It pops from `rtrb`,
converts to `i16`, and pushes `Arc<Chunk>` into a `tokio::sync::broadcast`
channel. One thread, bounded work, no timer jitter from the async scheduler.
`broadcast` also gives the right primitive for a slow receiver: `RecvError::Lagged(n)`
is exactly "this client fell behind", and the correct response is to flush that
client and send it a `Cut`.

## New dependencies

| Crate | For | Note |
| --- | --- | --- |
| `hyper` (server feature) | the HTTP GET that serves the page | already in `Cargo.lock` as a client via `reqwest`; this turns on the server half |
| `hyper-util` | the tokio glue hyper 1.x needs | |
| `tokio-tungstenite` | the WebSocket handshake and framing | hand-rolling the upgrade means hand-rolling SHA-1, masking and close handling |

Three additions, one of which is already compiled. The alternative — hand-write
the whole of a WebSocket server — trades three dependencies for a security-
relevant parser, which is the wrong trade.

`scripts/generate-notices.py` must be re-run; `THIRD-PARTY.md` is generated and
shipping without it distributes unlicensed code.

Not taken: `rubato` (a real resampler, the upgrade path for native sync), any
Opus encoder, any mDNS crate.

## Settings

```rust
struct Jam {
    port: u16,        // 8990, overridable with SONORA_JAM_PORT
    lead_ms: u32,     // 250
    name: String,     // "", meaning the host's display name
}
```

`Values` is `#[serde(default)]`, so an older `settings.json` gains the block and
needs no migration. Nothing about a running jam is persisted: a jam dies with
the process.

## UI

- **A third `SideTab`.** `SideTab` is `#[serde(rename_all = "lowercase")]` with
  two variants; a third is additive and an old file still parses. It sits beside
  Queue and Lyrics, reusing `Aside`'s tab machinery.
- **One more button in `PlayerBar::side_buttons`.** That function already builds
  its buttons from a closure taking `(id, icon, hint, side)`
  (`player_bar.rs:182`); this is one more call.
- **The panel**: idle shows a `ui::Vacancy` with a Start button, exactly like
  `shared::local::unconfigured`. Running shows the address, the code, the
  receiver list with a kick on each, the lead slider, and Stop. Joining shows an
  address field.
- **`jam-` in Fluent**, `assets/i18n/en-US/main.ftl` as the source. Receiver
  counts use a Fluent selector, never concatenation.
- **The icon**: no pack has one, and `radio.svg` already means the auto-radio
  feature. Add `radio-tower` to `assets/icons/lucide/` and to `MAP` in
  `scripts/fetch-icons.py`. A pack with no equivalent borrows Lucide's, which is
  the documented behaviour.

## Out of scope for v1

- **Opus or any compression.** Raw PCM only.
- **mDNS or any discovery.** Manual address.
- **Transport from a receiver.** A listener cannot play, pause, seek or skip.
  Adding to the queue is the one exception, and it is a setting (see below).
- **Browsing the host's library from a receiver.** Search answers with tracks
  and nothing else: no albums, no playlists, no saved items.
- **TLS**, and therefore the AudioWorklet path.
- **A variable-ratio resampler.** Frame drop and insert only.
- **Anything past the LAN.** No relay, no NAT traversal, no port mapping.
- **Non-Sonora endpoints.** No Chromecast, AirPlay, Snapcast, UPnP or DLNA.
- **Surviving a restart.** No jam is persisted.
- **IPv6.** The listener binds `0.0.0.0`.
- **Measuring the host's own output latency**, and therefore locking receivers
  to the host's own speakers.
- **Per-receiver volume from the host.** A receiver sets its own.

## Guests adding to the queue

A listener can search what the host can play and put a track at the end of the
queue. That is the whole of it: no reordering, no removing, no skipping to it.

The wire carries `Find { query }` and `Add { id }` from the receiver, and
`Found { query, hits }`, `Added { title }` and `Denied { reason }` back.

Three things make it safe enough for a home network:

- **It is a setting.** `jam.guests_add`, default on, because a jam nobody can
  add to is a speaker rather than a jam. Off makes the host answer `Denied`.
- **The host does the searching.** A listener never gets a client, a token or a
  provider session; it gets a list of titles and the ids belonging to them. The
  host resolves an id through its own `MusicApi` and appends the resulting
  `Track` — the same path the app's own queue uses.
- **Fifty adds per connection.** Past that the host answers `Denied { Busy }`.
  A reconnect resets it, which is fine: the limit is there to stop a stuck
  finger, not a determined guest who already has the code.

**The search does not block the audio.** A session's select loop never awaits a
host answer: it hands the question to the entity over a channel and keeps
streaming, and the answer arrives later through the session's own outbox. A
search that takes ten seconds costs a listener nothing but a spinner.

## Testing

This codebase tests pure functions at the bottom of the file they cover and has
no UI, network or audio harness. Three places qualify:

- **`music::cast`** — `f32` → `i16` conversion including clipping at both ends,
  the drop counter, and the chunk-boundary maths that turns a sample index into
  a chunk sequence.
- **`state::jam::wire`** — the binary header round-trips; a short or
  magic-less frame is refused; every text message round-trips; an unknown
  variant is an error rather than a default.
- **`state::jam::clock`** — the offset estimate from a known `(t0,t1,t2,t3)`,
  minimum-RTT selection, and the drift policy returning ignore, nudge or
  re-anchor at each boundary.

**Known gaps, accepted:** the tap itself, the server, the browser page and the
native receiver have no automated coverage. The tap runs on a realtime callback
thread, the rest needs sockets and a browser. Verification is the matrix in the
plan's last task, run against real devices, which is the only thing that proves
a multi-room feature anyway.

## Open questions

1. **The AudioWorklet is out; is scheduled `AudioBufferSourceNode` acceptable?**
   The reason is the secure-context rule, not preference. It changes nothing the
   user sees except that underrun handling is slightly coarser.
2. **Is 250 ms of lead the right default?** Lower means the host and the
   receivers diverge less on a pause and the room feels tighter; higher survives
   a worse network. It is a setting either way — which way should it default?
3. **1.4 Mbit/s per receiver, uncompressed.** Four receivers on 2.4 GHz will
   struggle. Ship v1 that way and add Opus when it bites, or plan the encoder
   now?
4. **Does the host also need a manual offset slider?** It does if the host's own
   room is part of the listening, because nothing measures its device latency.
   The proposal is yes, defaulted to zero.
5. **What happens to a running jam when the host signs out of the provider that
   is playing?** The proposal is: the stream goes `Quiet` and the jam stays open,
   because the next track may come from another provider.
