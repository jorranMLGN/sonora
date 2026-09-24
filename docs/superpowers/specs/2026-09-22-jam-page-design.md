# The jam page as a remote

Status: approved, decisions folded in
Branch: `feat/local-jam`
Audience: this fork only. Not intended for upstream.

Follows `2026-09-18-local-jam-design.md`, which built the page as a speaker. This
one turns it into a remote control for the room.

The audio half is out of bounds here. The tap, the chunker, the sample clock, the
lead negotiation and the scheduling loop in `page.html` are settled and this
design adds nothing to them. Everything below is additive: new wire variants, a
new watch channel, new sections on the page.

## What the page is today

A single column: artwork, title, artist, album, three badges, a progress bar that
counts forward locally between `Transport` messages, a Play/Stop button, a search
box that can append one track to the host's queue, and a card holding the device
trim plus diagnostics.

It can do exactly one thing to the host: `Find` then `Add`, gated by
`jam.guests_add`. Everything else is read-only.

## The feature set

Ranked by what a listener gets for what it costs. One through seven are in;
eight is out, because the grant model replaces it.

| # | Feature | Why | Cost | In |
| --- | --- | --- | --- | --- |
| 1 | **Transport** — play, pause, next, previous, seek, host volume | The named ask, and the reason the permission section exists. A phone that can only listen is a speaker. | Medium. One wire variant, one `Jam::act`, the capability check. | **Yes**, off by default |
| 2 | **This device's own volume and mute** | The gain node is already in the page. A phone in the kitchen wants to be quieter than the living room without touching anyone else. Costs nothing and removes the commonest reason to reach for the host's volume. | Tiny. A slider onto `gain.gain`, `localStorage`. | **Yes** |
| 3 | **The queue, live** | The named ask. See what is coming, tap a row to jump to it, swipe one away. It is also what makes adding a track feel like anything. | Medium. A bounded window, pushed. | **Yes** |
| 4 | **A name, and who else is in the room** | Today the host's panel shows a 40-character user-agent string per listener. A listener types a name once; everyone sees the roster. It is what makes a grant ("let the kitchen skip") mean anything. | Small. One field in `Hello`, one roster push. | **Yes** |
| 5 | **Playlists** | The named ask. Browse the host's playlists, open one, queue it or play it next. | High. Playlist summaries are in `Library`; their tracks are not, and fetching them is slow and paged. | **Yes** |
| 6 | **Favourite the playing track** | One tap, one wire pair, and `Library::toggle` already exists. Writes to the host's account, so it is a capability of its own. | Small. | **Yes** |
| 7 | **Synced lyrics** | `state::Lyrics` already resolves them. A phone lying on the table showing the words is the single nicest thing this page could do that the app cannot. | Medium. One push per track plus a line cursor. | **Yes** |
| 8 | **Requests instead of commands** | A guest's skip arrives at the host as a request to accept, rather than a skip. | Medium. | No. The grant model is the answer instead. |

Not proposed, deliberately: reordering the queue by drag, chat between
listeners, and anything that writes to the host's playlists.

## The permission question

### What exists

A six-digit code, in the URL path, checked by path match on both the page and
the WebSocket upgrade, with five strikes per address per minute in front of it.
One setting, `jam.guests_add`, default on, all-or-nothing across every listener.

Two things worth recording while we are here. `Hello.code` is never read — the
session destructures it away with `..`, because the path already carried it — and
so `Refusal::Code` is unreachable today. Neither is a bug; both mean a new field
on `Hello` is a cheap change.

### Why the code is not enough for transport

The code admits; it does not identify and it does not revoke. It is read aloud,
photographed off a screen, and never rotates for the life of the jam. Whoever
ever had it keeps it.

That is an acceptable gate for listening, because the worst outcome is that a
neighbour hears music already audible through the wall. It is not an acceptable
gate for transport, because the worst outcome is that someone pauses the host's
music in front of a room of people and the host cannot tell which device did it
or stop it doing it again.

So the code stays exactly what it is, and control gets its own gate on top.

### The model

**Capabilities, defaulted by a setting, granted per device, re-checked on every
message.**

```rust
enum Cap { Add, Control, Browse, Favorite }
```

- **The setting is the default a new listener gets.** `jam.guests` grows from a
  bool into a small struct with one flag per capability. `add` defaults on, as it
  does today. `control`, `browse` and `favorite` default **off** — a listener can
  hear and add without asking anyone, and pausing or seeking needs the host to
  grant that device. An older `settings.json` gains the block through
  `#[serde(default)]`, and the existing `jam.guests_add` key keeps its name and
  meaning so stored files survive.
- **A device can be promoted.** The jam panel lists each listener with its name
  and a control that grants or revokes each capability for that device alone. A
  grant is revocable, applies immediately, and is never persisted — it dies with
  the jam, like everything else about a jam.
- **Identity is a device token.** The page generates 128 random bits on first
  load, keeps them in `localStorage`, and sends them in `Hello` as `device`. The
  host keys grants, the roster and the kick by that token. The token is a handle,
  not a credential: anyone holding the device holds it, which is the same trust
  boundary the code already assumes.

  **This also fixes a live bug.** A listener is keyed today by
  `peer.to_string()` — an address and an *ephemeral* port. So a kicked phone
  only has to reload the page to get a new port and walk straight back in, and
  a reconnect after a screen lock shows up in the host's panel as a second
  listener. Keying on the token fixes both, and the host keeps the tokens it
  kicked for the life of the jam so a reload stays out, answered with a new
  `Refusal::Kicked`.
- **The page renders what it was granted.** `Grant { can }` is pushed after
  `Welcome` and again whenever it changes, and the page shows only the controls
  in it. This is display, not enforcement — the host re-checks every command, and
  a command from a device without the capability is answered
  `Denied { Forbidden }`, never ignored.
- **One panic control.** "Everyone back to listening" in the jam panel drops all
  grants at once, which is the thing a host actually reaches for.

### What the token is not: this all runs in the clear

The jam is served over plain HTTP and plain `ws://`, for the reasons the original
design gives — a LAN certificate no browser trusts costs a warning on every
device and buys nothing against an attacker already inside the building.

That applies to the grants as much as to the audio, and it has to be said rather
than left for a reader to work out:

- **The device token is a bearer credential the client mints itself.** The page
  generates 128 bits with `crypto.getRandomValues` and sends them in `Hello` in
  the clear, on every connect. Entropy is not the weak part; the wire is. Anyone
  who can watch the wifi can lift a token and replay it, and the host will hand
  them that device's grants.
- **So a grant is exactly as trustworthy as the network it is given on.** On a
  home network with a password, that is the same trust boundary the six-digit
  code already assumes. On an open or shared network, it is not a boundary at
  all.
- **What the token actually buys** is not secrecy. It is continuity and
  revocation: a grant survives a reconnect when a phone's screen locks, and a
  kicked device cannot come back by reloading the page. Both were broken before
  it, because the identity was an ephemeral TCP port.
- **TLS is the thing that would change this**, and it is out of scope here, as it
  was in the original design. Nothing else in this document should be read as
  defending against someone already on the network.

None of this makes the jam more exposed than it was: the audio was already
unencrypted and the code already travelled the same way. It does mean the
capability model is a control for the host's convenience and intent, not a
security boundary, and it should never be described as one.

### The trade-offs, said plainly

- **A per-capability boolean alone** (`jam.guests_control`) is one line of
  settings and answers the question wrongly: it means anyone on the LAN with the
  code can pause, which is the exact worry. Rejected as the whole answer, kept as
  the default layer.
- **Roles by name** (Guest / DJ) read better in a UI than a set of checkboxes,
  but they collapse the moment someone wants "can skip, cannot seek". The
  capability set is the honest model; the panel can still spell a full set as
  "DJ" if that reads better.
- **Approval per action** works for a skip and falls apart for a slider. A seek
  or a volume drag is a stream of intents and there is nothing to approve. Kept
  only as feature 8, for the hosts who want no grants at all.
- **Voting** is charming and degenerates at the common size: with two listeners
  any quorum is either unanimity or a dictatorship.

### Does this need more than the code?

For listening, no. For transport, the answer is not more gate but less reach:
control is off unless the host turns it on, and then only for a device the host
picked out of a named list. If that is still too loose for a given room, feature
8 is the fallback, and it needs no new security — only a different default
answer.

## The protocol

`PROTOCOL` goes to **5**. It reaches 4 in the lead-negotiation work; this is one
bump for the whole control surface rather than one per feature.

### Receiver to host

```rust
Hello    { …, device: String }         // new field, 128 bits hex, per browser
Command  { act: Act }                  // every transport move
Open     { pack: String }              // ask for one playlist's tracks
Enqueue  { pack: String, next: bool }  // append or play-next a playlist
Favorite { id: String, on: bool }      // the heart

enum Act {
    Play, Pause, Next, Previous,
    Seek { ms: u64 },
    Volume { level: f32 },
    Shuffle { on: bool },
    Repeat,
    Jump { index: i32 },
    Drop { index: u32 },
}
```

`Act::Repeat` cycles, because `Playback::cycle_repeat` cycles and the page should
not hold a second opinion about the order. `Act::Shuffle` carries the value
because shuffle lives on `Queue`, is a plain setter, and a toggle would race.
`Act::Jump` takes a signed index against the current track — negative into the
past, positive into what is upcoming — so one variant covers both of
`Playback::play_past` and `play_upcoming` and the page never has to say which
list it meant.

### Host to receiver

```rust
Grant    { can: Vec<Cap> }
Controls { volume: f32, shuffle: bool, repeat: Repeat,
           can_next: bool, can_previous: bool }
Lineup   { revision: u64, total: u32, from: i32, rows: Vec<Hit> }
Packs    { packs: Vec<Pack> }
Opened   { pack: String, rows: Vec<Hit> }
Room     { listeners: Vec<Who>, you: u32 }

struct Pack { id: String, name: String, owner: String,
              cover: Option<String>, tracks: u32, provider: String }
struct Who  { name: String, native: bool }
```

`Denial` gains `Forbidden` and `Refusal` gains `Kicked`. `Hit` is reused
unchanged for every track row: search
results, queue rows and playlist rows are the same shape to a page, and a second
track type on the wire would be two things to keep in step for no gain.

`Transport` is left alone. The transport's *shape* — volume, shuffle, repeat,
whether next and previous exist — changes rarely, and `Transport` fires twice a
second while playing. They are separate messages because they have separate
rates, and because the host dragging its own volume slider must not resend the
queue counts to every listener fifty times.

### The chattiness rules

The queue can be thousands of tracks and the same socket carries a 20 ms PCM
chunk fifty times a second. Two rules keep the one from starving the other.

**The host pushes a bounded window and never the whole queue.** `Lineup` carries
the current track plus at most the next fifty upcoming, with `total` saying how
many there really are and `from` saying where the window starts. A
five-thousand-track queue costs the same fifty rows as a five-track one, and the
page shows "and 4 950 more" from `total`.

This was the one contested call in the first draft, which proposed a pull
protocol — a small `Shape` push, and the page asking for the window it wanted.
It is not built. The window is bounded either way, so the pull only earns its
complexity for a listener scrolling a long queue, and it costs a visible flash
on every change for everyone else. `Lineup` carries `revision`, `total` and
`from` so a `Page`/`Rows` pull can be added later as new variants, without
changing anything that already exists on the wire.

If the push turns out to starve the audio in practice, that is a finding to
report, not a licence to build the pull protocol.

**Every push is coalesced before it reaches the socket.** All of `Lineup`,
`Controls`, `Packs`, `Room` and `Grant` come from one `watch` channel carrying
one snapshot (see below), and `watch` drops intermediate values by
construction — a session that was busy writing audio sees only the latest. On top
of that the snapshot is sent at most once per 100 ms. The concrete hazard this
closes is the host dragging a slider in its own window: `Playback::set_volume`
notifies on every frame, and without coalescing that is sixty pushes a second to
every listener.

Three smaller rules:

- **A window is fifty rows, clamped host-side.** That is roughly 10 KB of JSON,
  about 6 ms on 802.11n — well inside the lead.
- **No list is ever built inside the session's select loop.** The snapshot is
  built by the entity and a playlist's tracks come back through the existing
  spawn-and-reply path, so the loop's only new synchronous work is writing a
  finished line.
- **Commands are rate limited separately from adds.** The `ADDS` counter stays at
  fifty per connection for `Add` and `Enqueue`. Commands get a token bucket — 20
  per second, burst 40 — because a seek drag legitimately sends more messages in
  a minute than a guest will ever add tracks, and a wedged page must not be able
  to spin the entity.

### Cover art: a known limitation, left in place

Fifty rows means fifty cover URLs the browser fetches from the internet, from a
page that is explicitly served to a device with no assumed internet route. The
existing page already has this for `Now` and for search hits; a list multiplies
it.

This round does not fix it. Every list must therefore lay out correctly with no
covers at all, and that is a requirement on the page, not a hope. The fix is a
host-side route at `/c/<code>/art/<track id>` fed by `state::Cover` — keyed by
id, never by a URL the page supplies, because a proxy that fetches an arbitrary
URL on request is an open relay sitting on the host's network. It is its own
piece of work and it is not in this design.

## Where the data comes from

The existing bridge is `ServerEvent` plus a `oneshot` reply for receiver-to-host
questions, and a `watch` per pushed value for host-to-receiver. Both patterns
carry this design; one of them needs a shape change first.

### Asking the host

`ServerEvent` gains three variants, all in the shape of `Find` and `Add`:

```rust
Do   { act: Act, device: String, reply: oneshot::Sender<Answer> }
Open { pack: String, reply: oneshot::Sender<Vec<Hit>> }
Pick { pack: String, next: bool, reply: oneshot::Sender<Option<String>> }
```

The queue window, the playlist summaries, the controls and the roster do not
appear here: they are pushed from the snapshot, not asked for.

`Jam` answers them the way `state::remote` answers a media key: check, then call
`Playback` or `Queue`. That is the precedent to follow — the tray and the system
media controls already turn an outside event into `Playback` calls, and the jam
should read like a third such source, not like something new.

The mapping is mostly one call each. Worth naming the two that are not obvious:
**shuffle is on `Queue`, not `Playback`** (`Queue::set_shuffle`), and
`can_previous` comes from `Playback::has_previous(cx)` while `can_next` comes
from `Queue::has_next()`.

`Jam` needs `Entity<Library>` for playlists; it holds playback, cover, session,
queue and settings today, so `Jam::new` and its one call site in `lib.rs` gain a
parameter.

### Where the request/reply pattern does not stretch

**A command has nothing to wait for.** `Playback::pause` is infallible and
returns nothing. So the `oneshot` answers only *allowed* or *refused*, and the
page learns the *effect* from the state push that follows. Say it as a rule: a
command's acknowledgement is the resulting state, not a response. That is what
keeps a seek drag from costing a round trip per tick.

**Four listeners asking for the same playlist start four fetches.** Playlist
tracks are not in `Library` and come from `MusicApi` — slow, paged, and the same
answer for everyone. `Jam` keeps a per-jam cache plus an in-flight map, the shape
`Library::reading` and `Library::pending` already use, and the second asker joins
the first fetch instead of starting one.

**`ASK_WAIT` is ten seconds and that is right for a search and wrong for a
command.** A command that cannot be answered in a second should be refused, not
left hanging while the page's button stays greyed.

### Pushing to the listeners

This is the part that does not fit the existing shape, and the fix is small.

Today `Serving` carries one `watch` per pushed value — `now`, `transport`,
`leads` — and the session's select loop has one arm each. It is already at seven
arms. Adding `Lineup`, `Controls`, `Packs`, `Room` and `Grant` the same way means
five more, and five more channels to keep in step.

**Instead: one `watch<Arc<Snapshot>>`, one select arm, one diff.**

```rust
struct Snapshot {
    controls: Controls,
    lineup: Lineup,
    packs: Arc<Vec<Pack>>,
    room: Arc<Vec<Who>>,
    grants: HashMap<String, Caps>,
}
```

`Jam` rebuilds it in the `cx.observe` handlers it already has, plus new ones on
`Queue` and `Library`. The session holds the last snapshot it sent, compares
field by field, and emits only the messages that changed — `Controls` if the
transport shape moved, `Lineup` if the queue moved, `Room` if the roster moved,
`Grant` if *its own* entry in `grants` moved. Grants live in the shared snapshot
rather than a channel of their own precisely because a session can pick its own
row out of a map of at most eight.

`now`, `transport` and `leads` stay exactly where they are. This is one new field
on `Serving` and one new arm.

## The page

Still one page, still no build step, no framework, no bundler and no external
request. That is not negotiable and nothing below needs it: the whole thing is
DOM calls and a `switch`.

### Split it into three served files

`page.html` is 480 lines and would roughly triple. It becomes `page.html`,
`page.css` and `page.js`, each `include_str!`, each run through the same
`rendered()` substitution, served at `/c/<code>/app.css` and `/c/<code>/app.js`
with `cache-control: no-store`. Two extra arms in `answer()` and two extra
round trips on a LAN, against a JavaScript file an editor can actually lint.

`rendered()` is safe on all three: JavaScript template literals use `${}` and
CSS has no `{{`, so nothing collides with the placeholder syntax. The Fluent
table moves into `page.js`; the HTML keeps its `{{jam-page-*}}` placeholders.

Inside `page.js`, one object per screen — `now`, `queue`, `library`, `room` —
each with a `render()` and a message handler, over a shared `send()` and one
`switch` that dispatches. The audio path stays where it is and is not
reorganised around them.

### Navigation, phone first

A bottom tab bar with four tabs, and a one-line mini transport directly above it
that is hidden while **Now** is open.

| Tab | Holds |
| --- | --- |
| **Now** | Today's page: artwork, title, badges, progress. Plus the transport row, and the two volume sliders — *this device* and, when granted, *everyone* — labelled so they cannot be confused. |
| **Queue** | Now playing, then upcoming, then recently played. Tap a row to jump, swipe or a trailing control to remove. The search box moves here, because searching and queueing are the same job. |
| **Library** | The host's playlists. Open one for its tracks; queue it or play it next. Hidden entirely without `Cap::Browse`. |
| **Room** | The listener's own name, the roster, the device trim, and the diagnostics. |

The diagnostics move out of second position on the page and into Room. They are
still load-bearing for the reason the original design gave — a page that has
received everything and been tapped nowhere looks identical to a broken one — but
they are a debugging surface and should not be the first card a guest sees.

The page renders only the controls in its `Grant`, and re-renders when a `Grant`
arrives mid-session. A listener who is promoted while looking at the Now tab sees
the transport appear.

### Two smaller page changes

**The listener names itself.** Today `Hello.name` is
`navigator.userAgent.slice(0, 40)`, so the host's panel shows forty characters of
Mozilla boilerplate. A field in Room, remembered in `localStorage`, defaulting to
empty and shown to the host as the address until it is set.

**Strings.** Roughly thirty new `jam-page-*` keys in
`assets/i18n/en-US/main.ftl`, which is the source of truth, translated into ru,
uk and pl where someone can. Counts ("3 listening", "12 up next") use Fluent
selectors. No bare literal reaches the page: everything goes through a
placeholder and `i18n::lookup`, so the page follows the host's language the way
it already does.

## What this does not do

- **No writes to the host's playlists.** Reading them is a browse; adding to one
  is a mutation of the host's Spotify account through `Library::add_to_playlist`,
  at a trust level a six-digit code does not reach.
- **No reordering the queue by drag.** A drag on a phone, over a network, against
  a revision that may have moved is a great deal of complexity for a thing the
  host can do in the app. Jump and remove cover the intent.
- **No chat, no reactions, no listener-to-listener anything.** The roster is a
  list of names so a grant means something, not a social surface.
- **No accounts, no sign-in, no authentication.** The device token is a handle
  for grants and kicks. It is not a credential and must never be described as
  one.
- **No control over the host beyond playback.** Not its navigation, not its
  settings, not its providers, not sign-out.
- **No TLS**, unchanged, and therefore none of the secure-context browser
  features — no AudioWorklet, no service worker, no offline cache. The page is
  worthless without the host anyway.
- **Search stays tracks-only**, as the jam design set it. Albums and artists
  would each need their own row shape and their own fetch path; playlists are the
  one browse surface, and they come from `Library` for free.
- **No grants survive a jam.** Stopping and restarting starts everyone at the
  setting's defaults.
- **Nothing in the audio path changes.** No new field on `Welcome`, no change to
  `Mark`, `Ping`, `Pong` or `Lead`, no change to how a chunk is scheduled.

## Decisions

The questions this design opened have been answered. They are recorded here
rather than deleted, because the answers are the reason the shape above is what
it is.

1. **`Control` is off by default.** A listener can hear and add without asking.
   Pausing or seeking needs the host to grant that device. The six-digit code is
   not a credential and does not gate transport.
2. **The capability model is approved as designed** — `Add | Control | Browse |
   Favorite`, `jam.guests` for the default a newcomer gets, host promotes a
   specific device, revocable, never persisted.
3. **The device token is approved, and it is a bug fix as well as a feature.**
   Kick is broken today; the token is what repairs it. Kick and grant both key
   off the token, and a kicked token stays out for the life of the jam.
4. **Scope is everything except requests.** Features one through seven are in.
   Feature eight — requests instead of commands — is out; the grant model
   replaces it.
5. **The queue is pushed, not pulled.** Bounded window, no `Shape`/`Page`
   protocol, room left on the wire to add one later.
6. **`Refusal::Code` is fixed rather than removed.** It is unreachable today
   because the path already carried the code and `Hello.code` is destructured
   away with `..`. The session should check the code it was actually sent, so
   the WebSocket half validates its own payload rather than trusting the route
   that got it there.
7. **The cover proxy is out of this round**, written up above as a known
   limitation.

## Phasing

The work lands in phases, with a review between each.

| Phase | What |
| --- | --- |
| 1 | The protocol surface and the capability and token model: `Cap`, `Caps`, the settings block, `device` on `Hello`, the token-keyed roster and kick, `Refusal::Code`, `Refusal::Kicked`, `Denial::Forbidden`, the snapshot channel and its one select arm, and the host panel's grant controls. The page gains the token, its name field and nothing else. |
| 2 | Transport and per-device volume: `Command`, `Act`, `Controls`, `Jam::act`, and the page's tab shell with a transport row and the two volume sliders. |
| 3 | The live queue: `Lineup`, `Act::Jump`, `Act::Drop`, the queue list, and the roster it shares a row renderer with. |
| 4 | Playlists: `Packs`, `Open`, `Opened`, `Enqueue`, the Library tab, and the tab shell that tab makes necessary. |
| 5 | Favourite and lyrics. |
| 6 | The page split into `page.html`, `page.css` and `page.js`. |

The split is last on purpose. Doing it early means every later phase edits three
files instead of one; doing it late is one move with the content already
settled.

**A capability must be enforced by the phase that offers it.** `Cap::ALL` is what
the host panel shows a toggle for, and a capability only joins it once some host
-side path actually consults it. A toggle for a capability nothing checks is
worse than no toggle, because it reports a change that did not happen. It holds
`Add` and `Control` today; `Browse` joins in phase 4 and `Favorite` in phase 5.
