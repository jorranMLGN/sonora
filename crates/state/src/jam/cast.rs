use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use music::cast::{self, Feed};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::{broadcast, watch};

use super::wire::{Codec, Format, Header, MarkKind};

const CHUNK_MS: u32 = 20;
const IDLE: Duration = Duration::from_millis(4);
const QUIET_AFTER: Duration = Duration::from_millis(120);
const CHUNKS: usize = 64;

#[derive(Clone, Debug)]
pub struct Chunk {
    pub header: Header,
    pub bytes: Vec<u8>,
    pub mark: Option<MarkKind>,
}

pub struct Broadcast {
    live: Arc<Mutex<Option<&'static str>>>,
    format: watch::Sender<Option<Format>>,
    origin: Arc<AtomicU64>,
    cut: Arc<AtomicBool>,
    chunks: broadcast::Sender<Arc<Chunk>>,
}

impl Broadcast {
    pub fn new(feeds: UnboundedReceiver<Feed>) -> Self {
        let live = Arc::new(Mutex::new(None));
        let format = watch::channel(None).0;
        let origin = Arc::new(AtomicU64::new(now()));
        let cut = Arc::new(AtomicBool::new(false));
        let (chunks, _) = broadcast::channel(CHUNKS);

        let thread = Chunker {
            feeds,
            live: live.clone(),
            format: format.clone(),
            origin: origin.clone(),
            cut: cut.clone(),
            chunks: chunks.clone(),
        };
        let spawned = std::thread::Builder::new()
            .name("jam-chunker".to_owned())
            .spawn(move || thread.run());
        if let Err(error) = spawned {
            log::error!("jam: cannot spawn the chunker thread: {error}");
        }

        Self {
            live,
            format,
            origin,
            cut,
            chunks,
        }
    }

    pub fn follow(&self, slug: Option<&'static str>) {
        let mut live = match self.live.lock() {
            Ok(live) => live,
            Err(poisoned) => poisoned.into_inner(),
        };
        if *live == slug {
            return;
        }

        *live = slug;
        self.cut.store(true, Ordering::Release);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Chunk>> {
        self.chunks.subscribe()
    }

    pub fn format(&self) -> Option<Format> {
        *self.format.borrow()
    }

    pub fn formats(&self) -> watch::Receiver<Option<Format>> {
        self.format.subscribe()
    }

    pub fn origin(&self) -> u64 {
        self.origin.load(Ordering::Acquire)
    }
}

struct Chunker {
    feeds: UnboundedReceiver<Feed>,
    live: Arc<Mutex<Option<&'static str>>>,
    format: watch::Sender<Option<Format>>,
    origin: Arc<AtomicU64>,
    cut: Arc<AtomicBool>,
    chunks: broadcast::Sender<Arc<Chunk>>,
}

impl Chunker {
    fn run(mut self) {
        let mut open: HashMap<&'static str, Feed> = HashMap::new();
        let mut samples: Vec<f32> = Vec::new();
        let mut payload: Vec<u8> = Vec::new();
        let mut counter = 0u64;
        let mut seq = 0u32;
        let mut dry: Option<Instant> = None;
        let mut quiet = false;

        loop {
            match self.collect(&mut open) {
                Ok(()) => {}
                Err(()) => return,
            }

            let slug = self.slug();
            let Some(feed) = slug.and_then(|slug| open.get_mut(slug)) else {
                self.publish(None);
                std::thread::sleep(IDLE);
                continue;
            };

            self.publish(Some(Format {
                rate: feed.format.rate,
                channels: feed.format.channels,
                codec: Codec::Pcm16,
            }));

            if self.cut.swap(false, Ordering::AcqRel) {
                counter = 0;
                seq = 0;
                quiet = false;
                dry = None;
                samples.clear();
                self.origin.store(now(), Ordering::Release);
                self.send(&mut seq, counter, &[], Some(MarkKind::Cut));
            }

            let want = feed.format.samples(CHUNK_MS);
            while samples.len() < want {
                match feed.samples.pop() {
                    Ok(sample) => samples.push(sample),
                    Err(_) => break,
                }
            }

            if samples.len() < want {
                let since = *dry.get_or_insert_with(Instant::now);
                if !quiet && since.elapsed() >= QUIET_AFTER {
                    quiet = true;
                    self.send(&mut seq, counter, &[], Some(MarkKind::Quiet));
                }
                if quiet {
                    counter = counter.saturating_add(feed.format.frames(CHUNK_MS) as u64);
                    std::thread::sleep(Duration::from_millis(CHUNK_MS as u64));
                } else {
                    std::thread::sleep(IDLE);
                }
                continue;
            }

            let lost = feed.dropped.swap(0, Ordering::Relaxed);
            let mark = match (quiet, lost) {
                (false, 0) => None,
                _ => Some(MarkKind::Cut),
            };
            if lost > 0 {
                log::warn!("jam: dropped {lost} samples");
            }

            dry = None;
            quiet = false;
            cast::to_i16(&samples, &mut payload);
            let frames = samples.len() / feed.format.channels.max(1) as usize;
            samples.clear();

            self.send(&mut seq, counter, &payload, mark);
            counter = counter.saturating_add(frames as u64);
        }
    }

    fn collect(&mut self, open: &mut HashMap<&'static str, Feed>) -> Result<(), ()> {
        loop {
            match self.feeds.try_recv() {
                Ok(feed) => {
                    open.insert(feed.slug, feed);
                }
                Err(TryRecvError::Empty) => return Ok(()),
                Err(TryRecvError::Disconnected) => return Err(()),
            }
        }
    }

    fn slug(&self) -> Option<&'static str> {
        match self.live.lock() {
            Ok(live) => *live,
            Err(poisoned) => *poisoned.into_inner(),
        }
    }

    fn publish(&self, format: Option<Format>) {
        if *self.format.borrow() != format {
            self.format.send_replace(format);
        }
    }

    fn send(&self, seq: &mut u32, first_sample: u64, bytes: &[u8], mark: Option<MarkKind>) {
        let header = Header {
            seq: *seq,
            first_sample,
        };
        *seq = seq.wrapping_add(1);
        self.chunks
            .send(Arc::new(Chunk {
                header,
                bytes: bytes.to_vec(),
                mark,
            }))
            .ok();
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}
