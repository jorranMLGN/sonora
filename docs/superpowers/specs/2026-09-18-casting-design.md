# Casting To Devices That Do Not Run Sonora — Design

> Companion to [the jam design](2026-09-18-local-jam-design.md). The jam sends audio to
> Sonora and to browsers. This is about the speakers already in the house: Chromecast,
> AirPlay, DLNA, Sonos.

## What decides everything

Three of the four protocols work the same way: **the device fetches a URL you hand it.**
The sender is a remote control. Only AirPlay is different — there the sender encodes the
audio, encrypts it and owns the timing.

That single fact sets the order of work. One HTTP route that serves the jam's PCM as an
endless WAV unlocks Chromecast, DLNA and Sonos at once. AirPlay unlocks nothing else and
costs more than the other three together.

| | Crate or approach | Effort | Biggest risk | Reuses the jam server |
| --- | --- | --- | --- | --- |
| **HTTP audio route** | none, hand-written | 1–2 d | a format change mid-stream | yes |
| **Chromecast** | `rust_cast` 0.21 + `mdns-sd` 0.21 | 4–6 d | 1–3 s behind the host, always | yes |
| **DLNA/UPnP** | `rupnp` 3.0 + `ssdp-client` 2.1 | 3–5 d | header quirks per device | yes |
| **Sonos** | `rupnp` directly, **not** `sonor` | +1–2 d | `x-rincon-mp3radio://`, DIDL-Lite | yes |
| **AirPlay 1 (RAOP)** | no usable crate | 10–20 d | nothing published in Rust sends | no |
| **AirPlay 2** | idem, plus pairing crypto | 20–40 d | Apple moves the target | no |

## The route, and why WAV

`GET /c/<code>/stream.wav` on the jam's own port and behind the jam's own code: a 44-byte
RIFF header with `0xFFFFFFFF` for both sizes, then the same `i16` chunks the chunker already
produces for the WebSocket. No encoder, no new dependency, no second tap.

WAV is not a compromise here, it is the fastest option:

**A Chromecast's buffer is measured in bytes, not seconds.** `pulseaudio-dlna` reports 1–2 s
with WAV against ~5 s with MP3, and the project's own explanation is that "wav fills that
buffer much faster than efficient codecs do". LPCM at 1536 kbit/s fills it soonest of
anything a Chromecast accepts; FLAC halves the bandwidth and therefore **doubles** the delay.
The simplest container to write is also the quickest to start.

The second lever is `streamType: LIVE` rather than `BUFFERED`, with `duration: -1`.
`rust_cast`'s `StreamType` does carry a `Live` variant — verified in its docs, because the
recon could not confirm it and it decides whether this is 3 s or 20 s.

## Casting is not the jam

**A cast device can never be in step with the host, and pretending otherwise would break
the host.** The tap sits after the decoder, just before the host's own speakers: the host
hears a sample as it sends it. A Chromecast hears it 1–3 s later, in the good case. Closing
that gap means delaying the host's own playback by seconds, and with it every transport
control in the UI.

So casting is a **separate mode**, not another listener in the jam:

- A jam receiver is tens of milliseconds out, corrected by `Nudge::Slew` against a shared
  clock, and can be trimmed by ear with the offset slider.
- A cast device is seconds out and cannot be corrected. It is the right thing for "play this
  in the kitchen", and the wrong thing for "two speakers in one room".

The UI must say which of the two it is doing, rather than listing a Chromecast beside the
phones in the same list.

## Discovery

`mdns-sd` 0.21.3, and the reason is the warning in CLAUDE.md: `gpui_linux` runs `zbus` on
async-io, and mixing in `zbus/tokio` panics at runtime, which is why `ksni` must stay on
async-io. The lesson is not "pick async-io" — it is **pick something that brings no executor
at all**. `mdns-sd` describes itself exactly that way: it runs its own daemon thread and
talks over a `flume` channel that answers both `recv()` and `recv_async()`, so `state::Io`
can await it without either runtime owning it.

`flume`, `mio` and `socket2` are already in `Cargo.lock` at the versions it asks for.

Service names: `_googlecast._tcp.local` for Cast, `_raop._tcp.local` for AirPlay 1,
`_airplay._tcp.local` for AirPlay 2. DLNA uses SSDP over UDP 1900 instead and needs
`ssdp-client`.

## Why `rust_cast` and not the others

Its eight dependencies — `byteorder`, `log`, `protobuf =3.7.2`, `rustls`, `rustls-native-certs`,
`serde`, `serde_json`, `thiserror` — are all in the tree already, and `protobuf` sits on
exactly the 3.7.2 that its `=` pin demands, via `librespot-protocol`. Net new transitive
crates: about none.

Two things follow from the crate rather than the protocol:

- **It is synchronous.** No executor in its dependency set, so it belongs on its own thread
  or a blocking task, never on `io.spawn` beside async work.
- **Chromecasts carry a self-signed certificate**, and `connect_without_host_verification()`
  accepts any certificate. That is the normal route and practically the only one, but it is
  a deliberate decision and it is written down here rather than buried in a call site.

`cast-sender` is async but on smol plus `async-native-tls`, which drags in a second executor
and an OpenSSL system dependency this tree does not have. `chromecast` is an old fork.

## Why AirPlay waits

There is **no AirPlay sender crate on crates.io**. Everything published is a receiver. The
one serious Rust effort is alpha with `todo!()` stubs, and Music Assistant — a project with
every reason to have solved this — still bundles philippe44's C `libraop` after years.

A minimal RAOP sender means RTSP with an RSA-encrypted session key, ALAC encoding, RTP with
retransmission, and a timing channel. AirPlay 2 adds pairing and PTP on top, against firmware
Apple changes at will.

The honest sequence is: Chromecast first, measure whether a bare RAOP handshake is still
accepted by current firmware, and only then decide whether AirPlay is worth ten days or
forty.

## Out of scope

- **Casting video or artwork to a screen.** Audio only; a Cast receiver showing a still is
  a later nicety.
- **Group playback** (Cast groups, Sonos zones). One device at a time.
- **Controlling the device's own volume** from Sonora, beyond what the Cast media channel
  gives for free.
- **Anything past the LAN.**

## What has not been verified

- **Whether a Chromecast accepts chunked `audio/wav` with no `Content-Length`.** The
  indirect evidence is good — `pulseaudio-dlna` streams exactly this — but it is a
  twenty-line test against a real device and it gates the whole approach.
- **Whether `mdns-sd` coexists with Avahi or `systemd-resolved`** holding port 5353 on this
  machine. It binds its own multicast socket with `SO_REUSEADDR`/`SO_REUSEPORT`, which
  usually works, but it is a known friction point.
- **Whether current AirPlay 2 firmware still accepts the bare RAOP path.** This is the
  go/no-go for AirPlay and nothing else depends on it.
