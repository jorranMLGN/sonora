//! A track downloaded while it plays. The decoder reads from the front of one spool while the
//! response keeps filling the back, so playback starts after a short preroll instead of after
//! the whole file.
//!
//! The spool is an unlinked file in the cache directory, so a track sits in the page cache
//! rather than on the heap, and the kernel can drop it under pressure where it could never drop
//! a buffer. Memory is the fallback for a system where no such file can be made.
//!
//! Every provider that streams a file over HTTP uses this. What differs between them is where
//! the bytes come from, which is [`Source`], and what happens to them, which is [`Body`].
//! Subsonic and YouTube Music take them as they come, Deezer decrypts each Blowfish stripe as
//! it lands, and Apple Music indexes CENC fragments and hands each sample to a CDM the moment a
//! reader asks for it. Everything else, the waiting and the seeking and what a broken
//! connection does, is the same for all of them and lives here.
//!
//! A [`Ranged`] source can also start again at any byte. Its spool is filled out of order: a
//! read far from where the download is moves the download there, and once a run reaches the
//! end or bytes already in, the download goes back for the first hole left.
//!
//! A ranged spool can also be windowed, for a video nobody may watch to the end: its download
//! stays a bounded distance ahead of where it is being read, and never fetches what lies behind.

use std::borrow::Cow;
use std::fs::File;
use std::future::Future;
use std::io::{self, Read, Seek, SeekFrom};
use std::ops::Range;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use bytes::Bytes;
use tokio::sync::{Notify, watch};

/// How much has to be in before the decoder is let loose on the spool. At any ordinary bitrate
/// this is several seconds of audio, and the download outruns playback many times over, so the
/// lead only grows from here.
pub const PREROLL: usize = 256 * 1024;

/// How long a read waits for bytes that have not arrived. rodio pulls the decoder on the audio
/// thread, so a wait there is silence: bounded, because a dead connection must not hold the
/// output for the rest of the session.
const PATIENCE: Duration = Duration::from_secs(20);

/// The most a body of unknown length may spool. A long album side is tens of megabytes, so this
/// is only a ceiling on what an endless body can cost. A ranged body is bounded by the length it
/// announced instead. Small under test, so a test can run past it.
const CEILING: usize = match cfg!(test) {
    true => 4 * 1024 * 1024,
    false => 256 * 1024 * 1024,
};

/// How far past a ranged download's frontier a read waits for it rather than moving it. Within
/// this the running download gets there sooner than a new request would.
const LEAP: u64 = 2 * 1024 * 1024;

/// What a provider does to the body of one track: as it arrives, and before it is served.
///
/// Every method has an answer that suits a plain file, so a provider overrides only what it
/// actually does differently. `feed` and `flush` run on the tokio task pulling the response;
/// `limit`, `tail` and `ready` run on whichever thread is reading, under the spool's lock.
pub trait Body: Send + 'static {
    /// What to append for one chunk of the response. `out` starts empty on every call, and
    /// whatever is left in it goes onto the end of the spool. The default passes the chunk on
    /// unchanged.
    fn feed(&mut self, chunk: &[u8], out: &mut Vec<u8>) {
        out.extend_from_slice(chunk);
    }

    /// The response ended: append anything that was held back waiting for more.
    fn flush(&mut self, _out: &mut Vec<u8>) {}

    /// How far into the spool a read may be served. The default serves everything that has
    /// arrived; a provider that must account for bytes before handing them over answers less.
    ///
    /// The spool is mutable because accounting for bytes can mean preparing them: an fMP4 has
    /// its sample entry relabelled once its samples can be decrypted.
    fn limit(&mut self, spool: &mut Spool, _complete: bool) -> usize {
        spool.len()
    }

    /// How much of the end of the track a read may have without waiting for it.
    ///
    /// Zero, for a plain file: a read waits for what it asked for. A decoder that probes the
    /// end of a file before playing needs the last few kilobytes answered rather than waited
    /// on, or it holds playback until the whole track has arrived.
    fn tail(&self) -> u64 {
        0
    }

    /// Makes `range` readable, in place, before it is copied out. The default has nothing to do.
    fn ready(&mut self, _spool: &mut Spool, _range: Range<usize>) -> io::Result<()> {
        Ok(())
    }

    /// Whether the spool will hold decrypted media, which has to stay in memory and never
    /// reach a file, even an unlinked one whose pages get written back.
    fn confidential(&self) -> bool {
        false
    }

    /// The most this track may spool. A body whose tracks run longer than any ordinary file
    /// answers more, and its track may then reach the heap at that size if the disk fails it.
    fn ceiling(&self) -> usize {
        CEILING
    }
}

/// A body that is already what it should be.
pub struct Plain;

impl Body for Plain {}

/// Where the bytes of one track come from, in file order. A single response is one, and so is
/// a provider that asks for the file a range at a time.
pub trait Source: Send + 'static {
    /// The next part of the body, or `None` once it has all arrived.
    fn chunk(&mut self) -> impl Future<Output = Result<Option<Bytes>>> + Send;
}

/// A source that can start again at any byte of the same file, so a read far from where the
/// download has got to can have the download moved to it instead of waiting.
pub trait Ranged: Source + Sized {
    /// A new source over the same file whose first chunk starts at byte `at`. The original is
    /// left as it was.
    fn resumed(&self, at: u64) -> Self;
}

/// A failed read drops the url from its error, since a media url is often signed and the error
/// ends up in the log.
impl Source for reqwest::Response {
    async fn chunk(&mut self) -> Result<Option<Bytes>> {
        Ok(reqwest::Response::chunk(self)
            .await
            .map_err(reqwest::Error::without_url)?)
    }
}

/// The bytes of one track that have arrived so far, as a reader would see them. They live in
/// an unlinked file in the cache directory, which goes away with the last handle to it, or in
/// memory when no such file can be made or written or the body is [`Body::confidential`].
pub struct Spool {
    /// How many bytes from the start have arrived without a gap.
    len: usize,
    /// The body length the server announced, which a body can size its own bookkeeping by.
    announced: Option<usize>,
    store: Store,
    /// The ranges that have arrived, sorted and apart, for a spool filled out of order. `None`
    /// for one filled from the front only, where `len` says everything.
    filled: Option<Vec<Range<usize>>>,
}

enum Store {
    File(File),
    Memory(Vec<u8>),
}

impl Spool {
    /// A spool on disk, or in memory sized for `total` when the disk will not have one. The
    /// track may spool up to `ceiling` bytes.
    fn new(total: Option<u64>, ceiling: usize) -> Self {
        match spool_file() {
            Ok(file) => Self {
                len: 0,
                announced: announced(total, ceiling),
                store: Store::File(file),
                filled: None,
            },
            Err(error) => {
                log::warn!("playback: cannot spool to disk, buffering in memory: {error}");
                Self::held(total, ceiling)
            }
        }
    }

    /// A spool in memory for a track under the default ceiling, for a test to fill by hand.
    #[cfg(test)]
    pub(crate) fn in_memory(total: Option<u64>) -> Self {
        Self::held(total, CEILING)
    }

    /// A spool in memory, with room reserved for `total` bytes when that is known and for the
    /// preroll when it is not. A body of unknown length then starts out big enough to be mapped
    /// on its own, rather than growing through a run of heap blocks that each leave a hole. The
    /// reservation never passes the default ceiling, whatever `ceiling` the track may grow to.
    fn held(total: Option<u64>, ceiling: usize) -> Self {
        let room = total.map_or(PREROLL, |total| usize::try_from(total).unwrap_or(CEILING));
        Self {
            len: 0,
            announced: announced(total, ceiling),
            store: Store::Memory(Vec::with_capacity(room.min(CEILING))),
            filled: None,
        }
    }

    /// Lets an empty spool on disk be filled in any order. False for one in memory, which
    /// stays in order.
    fn scatter(&mut self) -> bool {
        let on_disk = matches!(self.store, Store::File(_)) && self.len == 0;
        if on_disk {
            self.filled = Some(Vec::new());
        }
        on_disk
    }

    /// The body length the server announced, if it did.
    pub fn announced(&self) -> Option<usize> {
        self.announced
    }

    /// How many bytes from the start have arrived, up to the first one that has not.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Fills `out` from position `at`. Asking for anything past what has arrived is an error.
    pub fn read_at(&self, at: usize, out: &mut [u8]) -> io::Result<()> {
        let end = self.check(at, out.len())?;
        match &self.store {
            Store::File(file) => read_exact_at(file, out, at as u64),
            Store::Memory(held) => {
                out.copy_from_slice(&held[at..end]);
                Ok(())
            }
        }
    }

    /// A copy of `range`, which must lie within what has arrived.
    pub fn bytes(&self, range: Range<usize>) -> io::Result<Vec<u8>> {
        let mut out = vec![0; range.end.saturating_sub(range.start)];
        self.read_at(range.start, &mut out)?;
        Ok(out)
    }

    /// The bytes of `range`, which must lie within what has arrived. A spool in memory lends
    /// them where they are, and only one on disk pays for a copy.
    pub fn view(&self, range: Range<usize>) -> io::Result<Cow<'_, [u8]>> {
        match &self.store {
            Store::Memory(held) => {
                let end = self.check(range.start, range.end.saturating_sub(range.start))?;
                Ok(Cow::Borrowed(&held[range.start..end]))
            }
            Store::File(_) => self.bytes(range).map(Cow::Owned),
        }
    }

    /// Overwrites the bytes at `at` with `bytes`, which must lie within what has arrived.
    pub fn write_at(&mut self, at: usize, bytes: &[u8]) -> io::Result<()> {
        let end = self.check(at, bytes.len())?;
        match &mut self.store {
            Store::File(file) => write_all_at(file, bytes, at as u64),
            Store::Memory(held) => {
                held[at..end].copy_from_slice(bytes);
                Ok(())
            }
        }
    }

    /// Adds `bytes` to the end. A disk that stops taking them moves the spool into memory
    /// rather than breaking the track.
    pub(crate) fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        if let Store::File(file) = &self.store {
            match write_all_at(file, bytes, self.len as u64) {
                Ok(()) => {
                    self.len += bytes.len();
                    return Ok(());
                }
                Err(error) => {
                    log::warn!("playback: cannot spool to disk, buffering in memory: {error}");
                    let mut held = vec![0; self.len];
                    read_exact_at(file, &mut held, 0)?;
                    self.store = Store::Memory(held);
                }
            }
        }
        if let Store::Memory(held) = &mut self.store {
            held.extend_from_slice(bytes);
            self.len = held.len();
        }
        Ok(())
    }

    /// Writes `bytes` at `at`, wherever that is, in a spool filled out of order. A disk that
    /// stops taking them moves the spool into memory, holes and all, rather than breaking the
    /// track.
    fn fill(&mut self, at: usize, bytes: &[u8]) -> io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        let end = at + bytes.len();
        if let Store::File(file) = &self.store {
            match write_all_at(file, bytes, at as u64) {
                Ok(()) => {
                    self.mark(at..end);
                    return Ok(());
                }
                Err(error) => {
                    log::warn!("playback: cannot spool to disk, buffering in memory: {error}");
                    let size = self.filled.iter().flatten().map(|r| r.end).max();
                    let mut held = vec![0; size.unwrap_or(0)];
                    read_exact_at(file, &mut held, 0)?;
                    self.store = Store::Memory(held);
                }
            }
        }
        if let Store::Memory(held) = &mut self.store {
            if held.len() < end {
                held.resize(end, 0);
            }
            held[at..end].copy_from_slice(bytes);
        }
        self.mark(at..end);
        Ok(())
    }

    /// Records `range` as arrived, merging it with its neighbours.
    fn mark(&mut self, range: Range<usize>) {
        let Some(filled) = self.filled.as_mut() else {
            return;
        };
        let (mut start, mut end) = (range.start, range.end);
        filled.retain(|r| {
            let apart = r.end < start || r.start > end;
            if !apart {
                start = start.min(r.start);
                end = end.max(r.end);
            }
            apart
        });
        let place = filled.partition_point(|r| r.end < start);
        filled.insert(place, start..end);
        self.len = filled.first().filter(|r| r.start == 0).map_or(0, |r| r.end);
    }

    /// Where the bytes that have arrived from `at` onwards without a gap end: `at` itself when
    /// that byte has not arrived.
    fn reach(&self, at: usize) -> usize {
        match &self.filled {
            Some(filled) => filled
                .iter()
                .find(|r| r.start <= at && at < r.end)
                .map_or(at, |r| r.end),
            None => self.len.max(at),
        }
    }

    /// The first byte at or after `from`, and before `total`, that has not arrived.
    fn hole(&self, from: usize, total: usize) -> Option<usize> {
        let mut at = from;
        for range in self.filled.iter().flatten() {
            if range.end <= at {
                continue;
            }
            if range.start > at {
                break;
            }
            at = range.end;
        }
        (at < total).then_some(at)
    }

    /// Where the first range that has arrived at or after `at` starts.
    fn next_filled(&self, at: usize) -> Option<usize> {
        self.filled
            .iter()
            .flatten()
            .find(|r| r.end > at)
            .map(|r| r.start.max(at))
    }

    /// The end of `len` bytes from `at`, when all of them have arrived.
    fn check(&self, at: usize, len: usize) -> io::Result<usize> {
        at.checked_add(len)
            .filter(|end| match self.filled {
                Some(_) => *end == at || *end <= self.reach(at),
                None => *end <= self.len,
            })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "a read runs past what has arrived",
                )
            })
    }
}

struct Buffered<B> {
    /// The track so far, as a reader would see it.
    spool: Spool,
    /// The body length the server announced, if it did.
    total: Option<u64>,
    /// The response finished, one way or the other.
    complete: bool,
    failed: Option<String>,
    body: B,
    /// Where a ranged download is writing. `None` for one that only fills from the front.
    run: Option<Run>,
}

/// Where the download of a scattered spool is, so a reader can tell whether waiting will
/// bring the byte it wants or the download has to move there.
struct Run {
    /// Where the current run started.
    start: usize,
    /// The next byte the current run will write.
    at: usize,
    /// Where a reader has asked the download to move, until the pump takes it.
    want: Option<usize>,
    /// Where the latest read ended, or where a read is waiting: what a windowed download keeps
    /// ahead of.
    read: usize,
    /// How far past `read` a windowed download may run. `None` downloads the whole file.
    window: Option<usize>,
    /// A windowed download has stopped, being far enough ahead of `read`, and waits for a
    /// reader to catch up.
    held: bool,
    /// A windowed download has nothing left to fetch past `read`, and waits for a reader to
    /// ask for a hole behind it.
    idle: bool,
}

impl Run {
    /// A run from the start of the file, `window` ahead of its reader when windowed.
    fn new(window: Option<usize>) -> Self {
        Self {
            start: 0,
            at: 0,
            want: None,
            read: 0,
            window,
            held: false,
            idle: false,
        }
    }

    /// Whether the current run will not reach `at`, unfilled, before a fresh request would:
    /// no run is going, or `at` lies behind where the run started, too far ahead of it, or
    /// past bytes that have already arrived, where the run stops.
    fn misses(&self, at: usize, spool: &Spool) -> bool {
        self.idle
            || at < self.start
            || at as u64 > self.at as u64 + LEAP
            || spool
                .next_filled(self.at)
                .is_some_and(|filled| filled <= at)
    }

    /// Records that a reader got to `at`, and says whether a held download should now go on.
    fn reached(&mut self, at: usize) -> bool {
        self.read = at;
        let Some(window) = self.window else {
            return false;
        };
        self.held && !self.idle && !holds(true, self.at, at, window)
    }
}

struct Shared<B> {
    state: Mutex<Buffered<B>>,
    filled: Condvar,
    /// Wakes a ranged pump when a reader asks it to move, or when the last handle goes.
    jump: Arc<Notify>,
}

impl<B> Drop for Shared<B> {
    fn drop(&mut self) {
        self.jump.notify_one();
    }
}

impl<B: Body> Shared<B> {
    fn held(&self) -> io::Result<MutexGuard<'_, Buffered<B>>> {
        self.state
            .lock()
            .map_err(|_| io::Error::other("the track buffer is poisoned"))
    }

    fn finish(&self, failed: Option<String>) {
        if let Ok(mut state) = self.state.lock() {
            let mut tail = Vec::new();
            state.body.flush(&mut tail);
            let spooled = state.spool.append(&tail);
            if state.failed.is_none() {
                state.failed = failed.or_else(|| spooled.err().map(|error| error.to_string()));
            }
            state.complete = true;
        }
        self.filled.notify_all();
    }
}

/// One download in progress. Clones share the bytes, and `reader` hands out an independent
/// cursor over them, so a preloaded track can be decoded more than once.
pub struct Stream<B = Plain> {
    shared: Arc<Shared<B>>,
    arrived: watch::Receiver<usize>,
}

/// A handle on a [`Stream`] that does not keep it alive, for background work that should stop
/// once nothing else wants the track. The download stops when the last strong handle goes,
/// whatever weak ones are left.
pub struct WeakStream<B = Plain> {
    shared: Weak<Shared<B>>,
    arrived: watch::Receiver<usize>,
}

impl<B> Clone for Stream<B> {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
            arrived: self.arrived.clone(),
        }
    }
}

impl<B: Body> Stream<B> {
    /// Starts pulling `response` into a spool and returns at once. The download carries on in
    /// the background for as long as any reader or clone is alive; it stops on its own once all
    /// of them are gone.
    pub fn new(response: reqwest::Response, body: B) -> Self {
        let total = response.content_length();
        Self::pulling(response, total, body)
    }

    /// Starts pulling `source` into a spool and returns at once, like [`new`](Self::new).
    /// `total` is the body length when it is known up front.
    pub fn pulling(source: impl Source, total: Option<u64>, body: B) -> Self {
        let spool = match body.confidential() {
            true => Spool::held(total, body.ceiling()),
            false => Spool::new(total, body.ceiling()),
        };
        Self::spooling(source, spool, total, body)
    }

    /// Starts pulling `source` in order into `spool`.
    fn spooling(source: impl Source, spool: Spool, total: Option<u64>, body: B) -> Self {
        let (shared, progress, stream) = Self::empty(spool, total, body, None);
        tokio::spawn(pump(source, Arc::downgrade(&shared), progress));
        stream
    }

    /// A stream over `spool` with nothing pulling into it yet, and the sender its pump reports
    /// progress on.
    fn empty(
        spool: Spool,
        total: Option<u64>,
        body: B,
        run: Option<Run>,
    ) -> (Arc<Shared<B>>, watch::Sender<usize>, Self) {
        let shared = Arc::new(Shared {
            state: Mutex::new(Buffered {
                spool,
                total,
                complete: false,
                failed: None,
                body,
                run,
            }),
            filled: Condvar::new(),
            jump: Arc::new(Notify::new()),
        });
        let (progress, arrived) = watch::channel(0usize);
        let stream = Self {
            shared: shared.clone(),
            arrived,
        };
        (shared, progress, stream)
    }
    /// Starts the download and waits for the preroll, or for the whole body of a track shorter
    /// than that.
    pub async fn open(response: reqwest::Response, body: B) -> Result<Self> {
        Self::new(response, body).primed().await
    }

    /// Waits for the preroll, or for the whole body of a track shorter than that.
    pub async fn primed(mut self) -> Result<Self> {
        let wanted = self.total().map_or(PREROLL, |total| {
            usize::try_from(total).unwrap_or(usize::MAX).min(PREROLL)
        });
        self.wait_for(wanted).await?;
        Ok(self)
    }

    /// Waits until the download has ended, whether it finished or broke.
    pub async fn finished(&self) {
        let mut arrived = self.arrived.clone();
        while !self.done() {
            if arrived.changed().await.is_err() {
                break;
            }
        }
    }

    /// Waits until `wanted` bytes have arrived, or the download ends. Nothing here blocks a
    /// thread: this is the async side of the same spool the readers wait on.
    pub async fn wait_for(&mut self, wanted: usize) -> Result<()> {
        loop {
            let arrived = *self.arrived.borrow_and_update();
            if arrived >= wanted || self.done() {
                break;
            }
            if self.arrived.changed().await.is_err() {
                break;
            }
        }
        match self.failed() {
            Some(failed) => bail!("the stream broke before playback could start: {failed}"),
            None => Ok(()),
        }
    }

    /// An independent cursor over the track.
    pub fn reader(&self) -> Reader<B> {
        Reader {
            shared: self.shared.clone(),
            head: 0,
            from: 0,
            at: 0,
        }
    }

    /// A cursor that serves the first `head` bytes and then carries on from `from`, so a
    /// decoder can be opened part way into a file that describes itself up front.
    pub fn spliced(&self, head: usize, from: usize) -> Reader<B> {
        Reader {
            shared: self.shared.clone(),
            head,
            from,
            at: 0,
        }
    }

    /// The body length the server announced, or the length it turned out to be once the
    /// download ended.
    pub fn total(&self) -> Option<u64> {
        let state = self.shared.state.lock().ok()?;
        state
            .total
            .or_else(|| state.complete.then_some(state.spool.len() as u64))
    }

    /// How much has arrived so far.
    pub fn arrived(&self) -> usize {
        self.shared
            .state
            .lock()
            .map(|state| state.spool.len())
            .unwrap_or(0)
    }

    /// Reaches the provider's own half of the spool, and again on every chunk that lands,
    /// until it answers something or the download ends. The third argument is whether the body
    /// is complete, so a question with no answer left can say so rather than wait.
    ///
    /// This blocks whichever thread asks, bounded by the same patience a read has. It is for a
    /// question a reader would otherwise have to guess at, where guessing is worse than
    /// waiting: where in the file a position is, when the download has not reached it yet.
    pub fn awaiting<T>(
        &self,
        mut read: impl FnMut(&mut B, &mut Spool, bool) -> Option<T>,
    ) -> Option<T> {
        let mut state = self.shared.state.lock().ok()?;
        let deadline = Instant::now() + PATIENCE;
        loop {
            let Buffered {
                spool,
                body,
                complete,
                failed,
                ..
            } = &mut *state;
            let (complete, broken) = (*complete, failed.is_some());
            if let Some(found) = read(body, spool, complete) {
                return Some(found);
            }
            if complete || broken {
                return None;
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            let (held, timed_out) = self.shared.filled.wait_timeout(state, left).ok()?;
            if timed_out.timed_out() {
                return None;
            }
            state = held;
        }
    }

    /// Reaches the provider's own half of the spool, for anything it keeps there.
    pub fn with<T>(&self, read: impl FnOnce(&mut B, &mut Spool) -> T) -> Option<T> {
        let mut state = self.shared.state.lock().ok()?;
        let Buffered { spool, body, .. } = &mut *state;
        Some(read(body, spool))
    }

    pub fn done(&self) -> bool {
        self.shared
            .state
            .lock()
            .map(|state| state.complete)
            .unwrap_or(true)
    }

    pub fn failed(&self) -> Option<String> {
        self.shared.state.lock().ok()?.failed.clone()
    }

    pub fn downgrade(&self) -> WeakStream<B> {
        WeakStream {
            shared: Arc::downgrade(&self.shared),
            arrived: self.arrived.clone(),
        }
    }
}

impl Stream<Plain> {
    /// Starts pulling `source` like [`pulling`](Stream::pulling), but lets a read far ahead of
    /// the download, or behind where it restarted, move the download there. The whole file is
    /// fetched in the end, so [`finished`](Self::finished) is reached.
    pub fn pulling_ranged(source: impl Ranged, total: Option<u64>) -> Self {
        Self::ranged(source, total, None)
    }

    /// [`pulling_ranged`](Self::pulling_ranged) that only fetches `window` bytes past where the
    /// stream is being read, and nothing behind it unless a read asks for it. For a video, which
    /// may be watched for a minute of an hour.
    ///
    /// Such a stream is [`finished`](Self::finished) only once every byte has been read for,
    /// which may be never, so nothing should wait on that. A body of unknown length cannot be
    /// windowed and is downloaded whole, in order, up to the ceiling.
    pub fn pulling_ranged_windowed(source: impl Ranged, total: Option<u64>, window: usize) -> Self {
        Self::ranged(source, total, Some(window.max(1)))
    }

    /// A ranged stream, windowed or not. A spool that cannot be scattered, in memory or for a
    /// body of unknown length, is filled in order.
    fn ranged<S: Ranged>(source: S, total: Option<u64>, window: Option<usize>) -> Self {
        let mut spool = Spool::new(total, Plain.ceiling());
        let Some(length) = total
            .and_then(|total| usize::try_from(total).ok())
            .filter(|_| spool.scatter())
        else {
            return Self::spooling(source, spool, total, Plain);
        };
        let (shared, progress, stream) = Self::empty(spool, total, Plain, Some(Run::new(window)));
        let jump = shared.jump.clone();
        let ranged = Ranges {
            current: source,
            at: 0,
            total: length,
            window,
            holding: false,
            idle: false,
        };
        tokio::spawn(ranged.pump(Arc::downgrade(&shared), jump, progress));
        stream
    }
}

impl<B: Body> WeakStream<B> {
    pub fn upgrade(&self) -> Option<Stream<B>> {
        Some(Stream {
            shared: self.shared.upgrade()?,
            arrived: self.arrived.clone(),
        })
    }

    /// Waits until the download has ended, whether it finished or broke, without keeping it
    /// alive in the meantime. False when every strong handle went first.
    pub async fn finished(&mut self) -> bool {
        loop {
            match self.upgrade() {
                Some(stream) if stream.done() => return true,
                Some(_) => {}
                None => return false,
            }
            if self.arrived.changed().await.is_err() {
                return self.upgrade().is_some_and(|stream| stream.done());
            }
        }
    }
}

/// Pulls the body into the spool, one chunk at a time, and stops as soon as nothing is left
/// that could read it.
async fn pump<S: Source, B: Body>(
    mut source: S,
    weak: Weak<Shared<B>>,
    progress: watch::Sender<usize>,
) {
    let mut staged = Vec::new();
    loop {
        let chunk = match source.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => {
                if let Some(shared) = weak.upgrade() {
                    shared.finish(None);
                }
                progress.send_modify(|_| {});
                return;
            }
            Err(error) => {
                log::warn!("playback: the stream broke: {error:#}");
                if let Some(shared) = weak.upgrade() {
                    shared.finish(Some(format!("{error:#}")));
                }
                progress.send_modify(|_| {});
                return;
            }
        };
        let Some(shared) = weak.upgrade() else {
            return;
        };
        let filled = {
            let Ok(mut state) = shared.state.lock() else {
                return;
            };
            let Buffered { spool, body, .. } = &mut *state;
            let ceiling = body.ceiling();
            if spool
                .len()
                .checked_add(chunk.len())
                .is_none_or(|end| end > ceiling)
            {
                drop(state);
                shared.finish(Some(format!("the track is longer than {ceiling} bytes")));
                progress.send_modify(|_| {});
                return;
            }
            staged.clear();
            body.feed(&chunk, &mut staged);
            if let Err(error) = spool.append(&staged) {
                drop(state);
                log::warn!("playback: cannot keep the track: {error}");
                shared.finish(Some(format!("cannot keep the track: {error}")));
                progress.send_modify(|_| {});
                return;
            }
            spool.len()
        };
        shared.filled.notify_all();
        progress.send(filled).ok();
    }
}

/// The download behind a scattered spool: one source writing at a time. A source the download
/// moves away from is dropped, never kept for later, since its `chunk` may have been cancelled
/// part way and so cannot be trusted to resume where it says it stopped.
struct Ranges<S> {
    current: S,
    /// The next byte `current` writes.
    at: usize,
    total: usize,
    /// How far past the reader a windowed download runs; `None` fetches the whole file.
    window: Option<usize>,
    /// The download is held, far enough ahead of the reader; see [`holds`].
    holding: bool,
    /// A windowed download with nothing left to fetch past the reader.
    idle: bool,
}

/// What woke a ranged pump.
enum Woken {
    Jump,
    Chunk(Result<Option<Bytes>>),
}

impl<S: Ranged> Ranges<S> {
    /// Pulls runs into the spool, moving to wherever a reader asks: until every byte has
    /// arrived, or, when windowed, only up to `window` past the reader, idling there until it
    /// comes closer. Stops as soon as nothing is left that could read it, the stream and every
    /// reader dropped.
    ///
    /// This one task is the only writer, and it never polls a source it has moved away from,
    /// so nothing a stopped run was fetching can land after the next run has started.
    async fn pump(
        mut self,
        weak: Weak<Shared<Plain>>,
        jump: Arc<Notify>,
        progress: watch::Sender<usize>,
    ) {
        loop {
            let paused = match weak.upgrade() {
                Some(shared) => self.pause(&shared),
                None => return,
            };
            // A pending jump wins over a chunk that is ready at the same time. `chunk` may be
            // dropped part way, which a source has to bear: its position moves only with a
            // chunk it hands out. A paused download polls no chunk at all, only for a reader.
            let woken = match paused {
                true => {
                    jump.notified().await;
                    Woken::Jump
                }
                false => tokio::select! {
                    biased;
                    () = jump.notified() => Woken::Jump,
                    chunk = self.current.chunk() => Woken::Chunk(chunk),
                },
            };
            let Some(shared) = weak.upgrade() else {
                return;
            };
            let moved = match woken {
                Woken::Jump => self.jump(&shared),
                Woken::Chunk(Ok(Some(chunk))) => self.write(&shared, &chunk),
                Woken::Chunk(Ok(None)) => match self.at < self.total {
                    true => Err(format!(
                        "the stream ended at byte {} of {}",
                        self.at, self.total
                    )),
                    false => self.next(&shared),
                },
                Woken::Chunk(Err(error)) => {
                    log::warn!("playback: the stream broke: {error:#}");
                    Err(format!("{error:#}"))
                }
            };
            match moved {
                Ok(true) => {
                    shared.filled.notify_all();
                    progress
                        .send(shared.held().map_or(0, |state| state.spool.len()))
                        .ok();
                }
                Ok(false) => {}
                Err(failed) => {
                    shared.finish(Some(failed));
                    progress.send_modify(|_| {});
                    return;
                }
            }
            if shared.held().is_ok_and(|state| state.complete) {
                progress.send_modify(|_| {});
                drop(shared);
                self.finish().await;
                return;
            }
        }
    }

    /// Asks a run that wrote the last byte for its end, which a source that keeps what passes
    /// through, as the vault's recording does, takes as the sign to seal it. A run that ended
    /// at a range that had arrived already is left alone, since it never reached the end.
    async fn finish(&mut self) {
        if self.at < self.total {
            return;
        }
        if let Ok(Ok(Some(_))) = tokio::time::timeout(PATIENCE, self.current.chunk()).await {
            log::debug!(
                "playback: the stream ran on past the {} bytes it announced",
                self.total
            );
        }
    }

    /// Whether a windowed download waits for its reader rather than pulling the next chunk:
    /// it is far enough ahead, or has nothing left past the reader. One that goes on after
    /// waiting asks again from where it stopped, since a connection left idle that long may
    /// have been dropped by the host.
    fn pause(&mut self, shared: &Shared<Plain>) -> bool {
        let Some(window) = self.window else {
            return false;
        };
        let Ok(mut state) = shared.held() else {
            return false;
        };
        let Some(run) = state.run.as_mut() else {
            return false;
        };
        let holding = holds(self.holding, self.at, run.read, window);
        if self.holding && !holding && !self.idle {
            log::debug!("playback: the download goes on from byte {}", self.at);
            self.current = self.source(self.at);
        }
        self.holding = holding;
        run.held = holding;
        run.idle = self.idle;
        holding || self.idle
    }

    /// Moves to where a reader asked, if that is still a hole. True when the run moved.
    fn jump(&mut self, shared: &Shared<Plain>) -> Result<bool, String> {
        let mut state = shared.held().map_err(|error| error.to_string())?;
        let Buffered { spool, run, .. } = &mut *state;
        let Some(run) = run.as_mut() else {
            return Ok(false);
        };
        let Some(to) = run
            .want
            .take()
            .and_then(|want| spool.hole(want, self.total))
        else {
            return Ok(false);
        };
        if to == self.at && !self.idle {
            return Ok(false);
        }
        log::debug!("playback: the download moves from byte {} to {to}", self.at);
        self.current = self.source(to);
        self.start(run, to);
        Ok(true)
    }

    /// Writes `chunk` where the run is, up to the end of the file or the next range that has
    /// arrived, where the run ends and the download moves to the first hole left.
    fn write(&mut self, shared: &Shared<Plain>, chunk: &[u8]) -> Result<bool, String> {
        if self.at + chunk.len() > self.total {
            return Err(format!(
                "the stream runs past the {} bytes it announced",
                self.total
            ));
        }
        let mut state = shared.held().map_err(|error| error.to_string())?;
        let Buffered { spool, run, .. } = &mut *state;
        let stop = spool.next_filled(self.at).unwrap_or(self.total);
        let kept = chunk.len().min(stop.saturating_sub(self.at));
        if let Err(error) = spool.fill(self.at, &chunk[..kept]) {
            log::warn!("playback: cannot keep the track: {error}");
            return Err(format!("cannot keep the track: {error}"));
        }
        self.at += kept;
        if let Some(run) = run.as_mut() {
            run.at = self.at;
        }
        let ended = self.at >= self.total || spool.next_filled(self.at) == Some(self.at);
        drop(state);
        if ended {
            self.next(shared)?;
        }
        Ok(true)
    }

    /// Starts a run at the first hole left, or marks the download complete when there is none.
    /// A windowed download looks only past its reader, and idles when nothing is left there.
    fn next(&mut self, shared: &Shared<Plain>) -> Result<bool, String> {
        let mut state = shared.held().map_err(|error| error.to_string())?;
        if state.spool.hole(0, self.total).is_none() {
            drop(state);
            shared.finish(None);
            return Ok(true);
        }
        let from = match (self.window, state.run.as_ref()) {
            (Some(_), Some(run)) => run.read,
            _ => 0,
        };
        let Some(hole) = state.spool.hole(from, self.total) else {
            self.idle = true;
            return Ok(true);
        };
        self.current = self.source(hole);
        if let Some(run) = state.run.as_mut() {
            self.start(run, hole);
        }
        Ok(true)
    }

    /// A fresh source for a run from `at`.
    fn source(&self, at: usize) -> S {
        self.current.resumed(at as u64)
    }

    /// Records that the run now starts at `at`.
    fn start(&mut self, run: &mut Run, at: usize) {
        self.at = at;
        self.holding = false;
        self.idle = false;
        run.start = at;
        run.at = at;
        run.held = false;
        run.idle = false;
    }
}

/// A cursor over a `Stream`. Reading past what has arrived blocks the caller until more does,
/// so a decoder simply waits out a slow connection.
pub struct Reader<B = Plain> {
    shared: Arc<Shared<B>>,
    /// The end of the part served from the front of the file, when this cursor is spliced.
    head: usize,
    /// Where the rest is served from. Zero for a plain cursor.
    from: usize,
    /// Where the cursor is, in what it presents rather than in the file.
    at: u64,
}

impl<B: Body> Reader<B> {
    /// The byte in the file that `at` presents, which differs only past the head of a splice.
    fn place(&self, at: u64) -> u64 {
        match at < self.head as u64 {
            true => at,
            false => self.from as u64 + (at - self.head as u64),
        }
    }

    /// A length in the file, as this cursor presents it.
    fn presented(&self, length: usize) -> u64 {
        self.head as u64 + length.saturating_sub(self.from) as u64
    }

    /// The end of the track: the announced length, or the final length once the download ends.
    /// A server that announced nothing makes this wait for the last byte.
    fn end(&self) -> u64 {
        let Ok(mut state) = self.shared.state.lock() else {
            return 0;
        };
        loop {
            if let Some(total) = state.total {
                return self.presented(total as usize);
            }
            if state.complete {
                return self.presented(state.spool.len());
            }
            let Ok((held, timed_out)) = self.shared.filled.wait_timeout(state, PATIENCE) else {
                return 0;
            };
            if timed_out.timed_out() {
                return self.presented(held.spool.len());
            }
            state = held;
        }
    }
}

impl<B: Body> Read for Reader<B> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        let mut state = self.shared.held()?;
        loop {
            if state.run.is_some() {
                match self.scattered(&mut state, out)? {
                    Some(read) => {
                        self.at += read as u64;
                        return Ok(read);
                    }
                    None => {
                        state = self.wait(state)?;
                        continue;
                    }
                }
            }
            let Buffered {
                spool,
                body,
                complete,
                total,
                ..
            } = &mut *state;
            let limit = self.presented(body.limit(spool, *complete));
            if self.at < limit {
                // A read never crosses a splice: the bytes on either side of it are nowhere
                // near each other in the file.
                let wanted = match self.at < self.head as u64 {
                    true => out.len().min(self.head - self.at as usize),
                    false => out.len(),
                };
                let start = self.place(self.at) as usize;
                let end = (start + wanted).min(self.place(limit) as usize);
                body.ready(spool, start..end)?;
                spool.read_at(start, &mut out[..end - start])?;
                self.at += (end - start) as u64;
                return Ok(end - start);
            }
            let ending = total
                .map(|total| self.presented(total as usize))
                .is_some_and(|total| self.at + body.tail() >= total && self.at < total);
            if let Some(failed) = &state.failed {
                return Err(io::Error::other(failed.clone()));
            }
            if state.complete || ending {
                return Ok(0);
            }
            state = self.wait(state)?;
        }
    }
}

impl<B: Body> Reader<B> {
    /// A read from a scattered spool: how much of what has arrived at the cursor was copied
    /// out, or `None` to wait for it. A byte the download will not reach soon on its own moves
    /// the download there. The caller moves the cursor.
    fn scattered(&self, state: &mut Buffered<B>, out: &mut [u8]) -> io::Result<Option<usize>> {
        let Buffered {
            spool,
            body,
            total,
            run,
            failed,
            complete,
            ..
        } = state;
        let start = self.place(self.at) as usize;
        if total.is_some_and(|total| start as u64 >= total) {
            return Ok(Some(0));
        }
        let reach = spool.reach(start);
        if reach > start {
            // A read never crosses a splice, as in the in-order case.
            let wanted = match self.at < self.head as u64 {
                true => out.len().min(self.head - self.at as usize),
                false => out.len(),
            };
            let end = start + wanted.min(reach - start);
            body.ready(spool, start..end)?;
            spool.read_at(start, &mut out[..end - start])?;
            if run.as_mut().is_some_and(|run| run.reached(end)) {
                self.shared.jump.notify_one();
            }
            return Ok(Some(end - start));
        }
        if let Some(failed) = failed {
            return Err(io::Error::other(failed.clone()));
        }
        if *complete {
            return Ok(Some(0));
        }
        if let Some(run) = run.as_mut() {
            let wake = run.reached(start);
            if run.misses(start, spool) && run.want != Some(start) {
                run.want = Some(start);
                self.shared.jump.notify_one();
            } else if wake {
                self.shared.jump.notify_one();
            }
        }
        Ok(None)
    }

    /// Waits for the spool to change, for as long as a read is patient.
    fn wait<'a>(
        &self,
        state: MutexGuard<'a, Buffered<B>>,
    ) -> io::Result<MutexGuard<'a, Buffered<B>>> {
        let (held, timed_out) = self
            .shared
            .filled
            .wait_timeout(state, PATIENCE)
            .map_err(|_| io::Error::other("the track buffer is poisoned"))?;
        if timed_out.timed_out() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the track stopped arriving",
            ));
        }
        Ok(held)
    }
}

impl<B: Body> Seek for Reader<B> {
    /// Free: nothing is waited on or made ready until something is read from the new position.
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::Current(delta) => i128::from(self.at) + i128::from(delta),
            SeekFrom::End(delta) => i128::from(self.end()) + i128::from(delta),
        };
        if target < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot seek before the start",
            ));
        }
        self.at = target as u64;
        Ok(self.at)
    }
}

/// Whether a windowed download at `at` holds off, its reader at `read`: it stops once it is
/// more than `window` ahead, and once stopped goes on only when the reader is within half a
/// window, so it fetches in long runs rather than a chunk each time the reader moves.
fn holds(holding: bool, at: usize, read: usize, window: usize) -> bool {
    let lead = at.saturating_sub(read);
    match holding {
        true => lead > window / 2,
        false => lead > window,
    }
}

/// An announced body length as a size, when it fits under `ceiling`.
fn announced(total: Option<u64>, ceiling: usize) -> Option<usize> {
    total
        .and_then(|total| usize::try_from(total).ok())
        .filter(|total| *total <= ceiling)
}

/// A file in Sonora's cache directory with no name, so nothing is left behind however the
/// process ends.
fn spool_file() -> io::Result<File> {
    let folder = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("sonora");
    std::fs::create_dir_all(&folder)?;
    tempfile::tempfile_in(folder)
}

#[cfg(unix)]
fn read_exact_at(file: &File, out: &mut [u8], at: u64) -> io::Result<()> {
    std::os::unix::fs::FileExt::read_exact_at(file, out, at)
}

#[cfg(unix)]
fn write_all_at(file: &File, bytes: &[u8], at: u64) -> io::Result<()> {
    std::os::unix::fs::FileExt::write_all_at(file, bytes, at)
}

/// Every caller holds the spool's lock, so moving the shared cursor cannot race another read.
#[cfg(not(unix))]
fn read_exact_at(mut file: &File, out: &mut [u8], at: u64) -> io::Result<()> {
    file.seek(SeekFrom::Start(at))?;
    file.read_exact(out)
}

#[cfg(not(unix))]
fn write_all_at(mut file: &File, bytes: &[u8], at: u64) -> io::Result<()> {
    use std::io::Write as _;
    file.seek(SeekFrom::Start(at))?;
    file.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stream without a response behind it, so the spool can be driven by hand.
    fn stream<B: Body>(body: B, total: Option<u64>) -> Stream<B> {
        let shared = Arc::new(Shared {
            state: Mutex::new(Buffered {
                spool: Spool::in_memory(total),
                total,
                complete: false,
                failed: None,
                body,
                run: None,
            }),
            filled: Condvar::new(),
            jump: Arc::new(Notify::new()),
        });
        let (_, arrived) = watch::channel(0usize);
        Stream { shared, arrived }
    }

    fn feed<B: Body>(stream: &Stream<B>, chunk: &[u8]) {
        let mut state = stream.shared.state.lock().unwrap();
        let Buffered { spool, body, .. } = &mut *state;
        let mut staged = Vec::new();
        body.feed(chunk, &mut staged);
        spool.append(&staged).unwrap();
    }

    fn finish<B: Body>(stream: &Stream<B>, failed: Option<String>) {
        stream.shared.finish(failed);
    }

    #[test]
    fn a_read_serves_what_arrived_and_then_ends() {
        let stream = stream(Plain, Some(8));
        let mut reader = stream.reader();
        feed(&stream, &[1, 2, 3, 4]);
        finish(&stream, None);

        let mut out = [0u8; 8];
        assert_eq!(reader.read(&mut out).unwrap(), 4);
        assert_eq!(&out[..4], &[1, 2, 3, 4]);
        assert_eq!(reader.read(&mut out).unwrap(), 0, "the body is complete");
    }

    #[test]
    fn seeking_is_absolute_and_clamped_at_zero() {
        let stream = stream(Plain, Some(100));
        finish(&stream, None);
        let mut reader = stream.reader();
        assert_eq!(reader.seek(SeekFrom::Start(5)).unwrap(), 5);
        assert_eq!(reader.seek(SeekFrom::Current(3)).unwrap(), 8);
        assert_eq!(reader.seek(SeekFrom::End(-10)).unwrap(), 90);
        assert!(reader.seek(SeekFrom::Current(-200)).is_err());
    }

    #[test]
    fn a_broken_download_surfaces_as_an_error() {
        let stream = stream(Plain, Some(8));
        let mut reader = stream.reader();
        finish(&stream, Some("the connection dropped".to_owned()));
        assert!(reader.read(&mut [0u8; 8]).is_err());
    }

    /// A body that accounts for bytes before serving them, the way a CENC index does.
    struct Half;

    impl Body for Half {
        fn limit(&mut self, spool: &mut Spool, _complete: bool) -> usize {
            spool.len() / 2
        }
    }

    #[test]
    fn a_body_can_hold_back_what_it_has_not_accounted_for() {
        let stream = stream(Half, None);
        let mut reader = stream.reader();
        feed(&stream, &[1, 2, 3, 4, 5, 6]);
        let mut out = [0u8; 6];
        assert_eq!(reader.read(&mut out).unwrap(), 3);
        assert_eq!(&out[..3], &[1, 2, 3]);
    }

    /// A body that transforms what arrives, the way a stripe cipher does.
    struct Doubling;

    impl Body for Doubling {
        fn feed(&mut self, chunk: &[u8], out: &mut Vec<u8>) {
            out.extend(chunk.iter().map(|byte| byte * 2));
        }

        fn flush(&mut self, out: &mut Vec<u8>) {
            out.push(255);
        }
    }

    #[test]
    fn a_body_shapes_the_bytes_as_they_arrive() {
        let stream = stream(Doubling, None);
        let mut reader = stream.reader();
        feed(&stream, &[1, 2, 3]);
        finish(&stream, None);
        let mut out = Vec::new();
        reader.read_to_end(&mut out).unwrap();
        assert_eq!(out, vec![2, 4, 6, 255], "fed, then flushed");
    }

    /// Without this a decoder probing the end of the file would hold playback until the whole
    /// track had arrived. A plain body waits instead, which is why it is not the default.
    #[test]
    fn a_tail_read_does_not_wait_when_the_body_allows_it() {
        struct Probing;
        impl Body for Probing {
            fn tail(&self) -> u64 {
                8
            }
        }

        let stream = stream(Probing, Some(64));
        let mut reader = stream.reader();
        reader.seek(SeekFrom::End(-4)).unwrap();
        assert_eq!(reader.read(&mut [0u8; 4]).unwrap(), 0);
    }

    /// A spliced cursor presents the head of the file followed by a later part of it, which is
    /// what lets a decoder open at a position instead of reading its way there.
    #[test]
    fn a_spliced_cursor_joins_the_head_to_a_later_part() {
        let stream = stream(Plain, Some(10));
        feed(&stream, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        finish(&stream, None);

        let mut reader = stream.spliced(3, 7);
        let mut out = Vec::new();
        reader.read_to_end(&mut out).unwrap();
        assert_eq!(out, vec![0, 1, 2, 7, 8, 9]);
        // And it reports its own length, not the file's.
        assert_eq!(reader.seek(SeekFrom::End(0)).unwrap(), 6);
    }

    const MIB: usize = 1024 * 1024;

    /// A file served from memory 64 KiB at a time, which logs every restart and, when gated,
    /// hands out one chunk per permit.
    struct Fake {
        data: Arc<Vec<u8>>,
        at: u64,
        log: Arc<Mutex<Vec<u64>>>,
        gate: Option<Arc<tokio::sync::Semaphore>>,
    }

    impl Source for Fake {
        async fn chunk(&mut self) -> Result<Option<Bytes>> {
            let start = self.at as usize;
            if start >= self.data.len() {
                return Ok(None);
            }
            if let Some(gate) = &self.gate {
                gate.acquire().await?.forget();
            }
            let end = (start + 64 * 1024).min(self.data.len());
            self.at = end as u64;
            Ok(Some(Bytes::copy_from_slice(&self.data[start..end])))
        }
    }

    impl Ranged for Fake {
        fn resumed(&self, at: u64) -> Self {
            self.log.lock().unwrap().push(at);
            Self {
                data: self.data.clone(),
                at,
                log: self.log.clone(),
                gate: None,
            }
        }
    }

    /// A file of `len` bytes that differ from their neighbours, and a source over it that lets
    /// through only the first `permits` chunks until its gate is opened further.
    fn fake(len: usize, permits: usize) -> (Fake, Arc<Vec<u8>>, Arc<tokio::sync::Semaphore>) {
        let data: Arc<Vec<u8>> = Arc::new((0..len).map(|i| (i % 251) as u8).collect());
        let gate = Arc::new(tokio::sync::Semaphore::new(permits));
        let source = Fake {
            data: data.clone(),
            at: 0,
            log: Arc::default(),
            gate: Some(gate.clone()),
        };
        (source, data, gate)
    }

    fn read_at<B: Body>(stream: &Stream<B>, at: usize, len: usize) -> Vec<u8> {
        let mut reader = stream.reader();
        reader.seek(SeekFrom::Start(at as u64)).unwrap();
        let mut out = vec![0; len];
        reader.read_exact(&mut out).unwrap();
        out
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_read_far_ahead_restarts_the_download_there() {
        let (source, data, _gate) = fake(16 * MIB, 16);
        let log = source.log.clone();
        let mut stream = Stream::pulling_ranged(source, Some(16 * MIB as u64));
        stream.wait_for(MIB).await.unwrap();

        let out = read_at(&stream, 12 * MIB, 64 * 1024);
        assert_eq!(out, data[12 * MIB..12 * MIB + 64 * 1024]);
        assert_eq!(*log.lock().unwrap(), vec![12 * MIB as u64]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_read_just_ahead_waits_instead_of_restarting() {
        let (source, data, gate) = fake(16 * MIB, 16);
        let log = source.log.clone();
        let mut stream = Stream::pulling_ranged(source, Some(16 * MIB as u64));
        stream.wait_for(MIB).await.unwrap();

        let reading = std::thread::spawn({
            let stream = stream.clone();
            move || read_at(&stream, MIB + 512 * 1024, 64 * 1024)
        });
        std::thread::sleep(Duration::from_millis(100));
        gate.add_permits(1024);
        let out = reading.join().unwrap();
        assert_eq!(out, data[MIB + 512 * 1024..MIB + 576 * 1024]);
        assert!(log.lock().unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn bytes_before_a_jump_stay_readable() {
        let (source, data, _gate) = fake(16 * MIB, 16);
        let log = source.log.clone();
        let mut stream = Stream::pulling_ranged(source, Some(16 * MIB as u64));
        stream.wait_for(MIB).await.unwrap();
        read_at(&stream, 12 * MIB, 64 * 1024);

        assert_eq!(read_at(&stream, 0, MIB), data[..MIB]);
        assert_eq!(*log.lock().unwrap(), vec![12 * MIB as u64]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_read_into_an_earlier_hole_restarts_there() {
        let (source, data, _gate) = fake(16 * MIB, 16);
        let log = source.log.clone();
        let mut stream = Stream::pulling_ranged(source, Some(16 * MIB as u64));
        stream.wait_for(MIB).await.unwrap();
        read_at(&stream, 12 * MIB, 64 * 1024);

        assert_eq!(
            read_at(&stream, 6 * MIB, 64 * 1024),
            data[6 * MIB..6 * MIB + 64 * 1024]
        );
        assert_eq!(*log.lock().unwrap(), vec![12 * MIB as u64, 6 * MIB as u64]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_holes_fill_once_the_end_is_reached() {
        let (source, data, gate) = fake(16 * MIB, 16);
        let mut stream = Stream::pulling_ranged(source, Some(16 * MIB as u64));
        stream.wait_for(MIB).await.unwrap();
        read_at(&stream, 12 * MIB, 64 * 1024);
        gate.add_permits(4096);

        tokio::time::timeout(Duration::from_secs(10), stream.finished())
            .await
            .expect("the download finishes");
        assert!(stream.done());
        assert_eq!(stream.failed(), None);
        let mut out = Vec::new();
        stream.reader().read_to_end(&mut out).unwrap();
        assert!(out == *data, "every byte is in place");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_ranged_stream_is_bounded_by_its_length_not_the_ceiling() {
        // one run, from the start to the end, longer than the ceiling
        let len = CEILING + 2 * MIB;
        let (source, data, _gate) = fake(len, 4096);
        let log = source.log.clone();
        let stream = Stream::pulling_ranged(source, Some(len as u64));

        tokio::time::timeout(Duration::from_secs(10), stream.finished())
            .await
            .expect("the download finishes");
        assert_eq!(stream.failed(), None);
        assert!(log.lock().unwrap().is_empty(), "in one run");
        let mut out = Vec::new();
        stream.reader().read_to_end(&mut out).unwrap();
        assert!(out == *data, "every byte is in place");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_body_of_unknown_length_still_stops_at_the_ceiling() {
        let (source, _data, _gate) = fake(CEILING + MIB, 4096);
        let stream = Stream::pulling_ranged(source, None);
        tokio::time::timeout(Duration::from_secs(10), stream.finished())
            .await
            .expect("the download ends");
        assert!(
            stream
                .failed()
                .is_some_and(|failed| failed.contains("longer than"))
        );
    }

    /// How far the download has filled from `at` without a gap, once it has stopped moving.
    async fn settled(stream: &Stream, at: usize) -> usize {
        let reach = || stream.with(|_, spool| spool.reach(at)).unwrap();
        let mut last = reach();
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let now = reach();
            if now == last {
                return now;
            }
            last = now;
        }
        panic!("the download never stopped");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_windowed_stream_stays_a_window_ahead_of_its_reader() {
        let (source, data, _gate) = fake(16 * MIB, 4096);
        let log = source.log.clone();
        let stream = Stream::pulling_ranged_windowed(source, Some(16 * MIB as u64), 2 * MIB);
        assert_eq!(read_at(&stream, 0, 64 * 1024), data[..64 * 1024]);

        let held = settled(&stream, 0).await;
        assert!(held > 2 * MIB, "it runs a window ahead, got {held}");
        assert!(held <= 2 * MIB + 128 * 1024, "and stops there, got {held}");

        // more than half a window ahead of the reader it still holds
        read_at(&stream, held - 3 * MIB / 2 - 64 * 1024, 64 * 1024);
        assert_eq!(settled(&stream, 0).await, held);
        assert!(log.lock().unwrap().is_empty());

        // past that it goes on, from where it stopped, and holds again a window on
        let at = held - 64 * 1024;
        assert_eq!(read_at(&stream, at, 64 * 1024), data[at..held]);
        let resumed = settled(&stream, 0).await;
        assert!(
            resumed > held + 2 * MIB - 64 * 1024,
            "it went on, got {resumed}"
        );
        assert!(
            resumed <= held + 2 * MIB + 128 * 1024,
            "a window on, got {resumed}"
        );
        assert_eq!(*log.lock().unwrap(), vec![held as u64]);
        assert!(!stream.done(), "a windowed stream need never finish");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_windowed_stream_restarts_where_a_read_jumps_beyond_it() {
        let (source, data, _gate) = fake(16 * MIB, 4096);
        let log = source.log.clone();
        let stream = Stream::pulling_ranged_windowed(source, Some(16 * MIB as u64), 2 * MIB);
        read_at(&stream, 0, 64 * 1024);
        settled(&stream, 0).await;

        let far = 12 * MIB;
        assert_eq!(read_at(&stream, far, 64 * 1024), data[far..far + 64 * 1024]);
        let reach = settled(&stream, far).await;
        assert!(
            reach <= far + 2 * MIB + 192 * 1024,
            "a window past the jump, got {reach}"
        );
        assert_eq!(log.lock().unwrap().first(), Some(&(far as u64)));
        // the hole between the first window and the jump is left alone
        assert_eq!(
            stream.with(|_, spool| spool.reach(4 * MIB)).unwrap(),
            4 * MIB
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_windowed_stream_idles_at_the_end_and_fills_a_hole_asked_for() {
        let (source, data, _gate) = fake(8 * MIB, 4096);
        let log = source.log.clone();
        let stream = Stream::pulling_ranged_windowed(source, Some(8 * MIB as u64), MIB);
        read_at(&stream, 0, 64 * 1024);
        settled(&stream, 0).await;
        // more than a leap ahead, near enough the end that the run reaches it
        read_at(&stream, 7 * MIB, 64 * 1024);
        assert_eq!(settled(&stream, 7 * MIB).await, 8 * MIB);
        assert!(!stream.done(), "the hole behind is not fetched");

        let hole = stream.with(|_, spool| spool.reach(0)).unwrap();
        assert_eq!(
            read_at(&stream, hole, 64 * 1024),
            data[hole..hole + 64 * 1024]
        );
        assert_eq!(*log.lock().unwrap(), vec![7 * MIB as u64, hole as u64]);
    }

    #[test]
    fn a_held_download_goes_on_only_within_half_a_window() {
        let window = 100;
        assert!(!holds(false, 150, 50, window));
        assert!(holds(false, 151, 50, window));
        assert!(holds(true, 151, 100, window));
        assert!(!holds(true, 150, 100, window));
        // a reader ahead of the download never holds it
        assert!(!holds(true, 10, 500, window));
    }
}
