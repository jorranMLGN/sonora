use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rtrb::RingBuffer;

const RING_MILLIS: u32 = 1_000;
const SCALE: f32 = i16::MAX as f32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    pub rate: u32,
    pub channels: u16,
}

pub struct Cast {
    producer: rtrb::Producer<f32>,
    dropped: Arc<AtomicU64>,
}

pub struct Feed {
    pub slug: &'static str,
    pub format: Format,
    pub samples: rtrb::Consumer<f32>,
    pub dropped: Arc<AtomicU64>,
}

pub type CastSink = Arc<dyn Fn(Feed) + Send + Sync>;

impl Format {
    pub fn frames(&self, millis: u32) -> usize {
        (self.rate as u64 * millis as u64 / 1_000) as usize
    }

    pub fn samples(&self, millis: u32) -> usize {
        self.frames(millis) * self.channels.max(1) as usize
    }
}

impl Cast {
    pub fn push(&mut self, sample: f32) {
        if self.producer.push(sample).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub fn open(slug: &'static str, format: Format) -> (Cast, Feed) {
    let (producer, samples) = RingBuffer::<f32>::new(format.samples(RING_MILLIS).max(1));
    let dropped = Arc::new(AtomicU64::new(0));
    let cast = Cast {
        producer,
        dropped: dropped.clone(),
    };
    let feed = Feed {
        slug,
        format,
        samples,
        dropped,
    };
    (cast, feed)
}

pub fn to_i16(samples: &[f32], out: &mut Vec<u8>) {
    out.clear();
    out.reserve(samples.len() * 2);
    for sample in samples {
        // saturating cast clips
        let scaled = (sample * SCALE) as i16;
        out.extend_from_slice(&scaled.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sample_becomes_little_endian_i16() {
        let mut out = Vec::new();
        to_i16(&[0.0, 0.5, -0.5], &mut out);
        assert_eq!(out, vec![0, 0, 0xFF, 0x3F, 0x01, 0xC0]);
    }

    #[test]
    fn conversion_clips_instead_of_wrapping() {
        let mut out = Vec::new();
        to_i16(&[2.0, -2.0, f32::INFINITY, f32::NEG_INFINITY], &mut out);
        let values: Vec<i16> = out
            .chunks_exact(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        assert_eq!(values, vec![i16::MAX, i16::MIN, i16::MAX, i16::MIN]);
    }

    #[test]
    fn a_nan_becomes_silence_rather_than_noise() {
        let mut out = Vec::new();
        to_i16(&[f32::NAN], &mut out);
        assert_eq!(out, vec![0, 0]);
    }

    #[test]
    fn a_chunk_is_a_whole_number_of_frames() {
        let format = Format {
            rate: 44_100,
            channels: 2,
        };
        assert_eq!(format.frames(20), 882);
        assert_eq!(format.samples(20), 1_764);
        let format = Format {
            rate: 48_000,
            channels: 2,
        };
        assert_eq!(format.frames(20), 960);
    }

    #[test]
    fn a_full_ring_counts_what_it_drops() {
        let format = Format {
            rate: 8,
            channels: 1,
        };
        let (mut cast, feed) = open("test", format);
        for _ in 0..10_000 {
            cast.push(0.25);
        }
        assert!(feed.dropped.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn a_ring_that_keeps_up_drops_nothing() {
        let format = Format {
            rate: 8,
            channels: 1,
        };
        let (mut cast, mut feed) = open("test", format);
        for _ in 0..4 {
            cast.push(0.25);
            feed.samples.pop().ok();
        }
        assert_eq!(feed.dropped.load(Ordering::Relaxed), 0);
    }
}
