use std::collections::VecDeque;

pub const SAMPLES: usize = 8;

const HELD: i64 = 2;
const ANCHOR: i64 = 100;
const PPM_PER_MS: i64 = 20;
const MAX_PPM: i64 = 2_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub t0: u64,
    pub t1: u64,
    pub t2: u64,
    pub t3: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nudge {
    Hold,
    Slew(i32),
    Anchor,
}

#[derive(Default)]
pub struct Clock {
    samples: VecDeque<Sample>,
}

impl Sample {
    pub fn offset(&self) -> i64 {
        let forward = (self.t1 as i64).saturating_sub(self.t0 as i64);
        let back = (self.t2 as i64).saturating_sub(self.t3 as i64);
        forward.saturating_add(back) / 2
    }

    pub fn rtt(&self) -> u64 {
        self.t3
            .saturating_sub(self.t0)
            .saturating_sub(self.t2.saturating_sub(self.t1))
    }
}

impl Clock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, sample: Sample) {
        self.samples.push_back(sample);
        while self.samples.len() > SAMPLES {
            self.samples.pop_front();
        }
    }

    pub fn best(&self) -> Option<Sample> {
        self.samples.iter().copied().min_by_key(Sample::rtt)
    }

    pub fn offset(&self) -> i64 {
        self.best().map(|sample| sample.offset()).unwrap_or(0)
    }

    pub fn rtt(&self) -> u64 {
        self.best().map(|sample| sample.rtt()).unwrap_or(0)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.samples.len()
    }
}

pub fn play_at(origin: u64, first_sample: u64, rate: u32, lead: u32) -> u64 {
    let elapsed = first_sample.saturating_mul(1_000) / rate.max(1) as u64;
    origin.saturating_add(elapsed).saturating_add(lead as u64)
}

pub fn correction(error_ms: i64) -> Nudge {
    match error_ms.abs() {
        held if held <= HELD => Nudge::Hold,
        anchor if anchor >= ANCHOR => Nudge::Anchor,
        _ => Nudge::Slew(error_ms.saturating_mul(PPM_PER_MS).clamp(-MAX_PPM, MAX_PPM) as i32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_offset_from_a_symmetric_round_trip() {
        let sample = Sample {
            t0: 0,
            t1: 1_010,
            t2: 1_010,
            t3: 20,
        };
        assert_eq!(sample.offset(), 1_000);
        assert_eq!(sample.rtt(), 20);
    }

    #[test]
    fn a_host_processing_delay_does_not_move_the_offset() {
        let sample = Sample {
            t0: 0,
            t1: 1_010,
            t2: 1_110,
            t3: 120,
        };
        assert_eq!(sample.offset(), 1_000);
        assert_eq!(sample.rtt(), 20);
    }

    #[test]
    fn the_least_queued_sample_wins() {
        let mut clock = Clock::new();
        clock.push(Sample {
            t0: 0,
            t1: 1_100,
            t2: 1_100,
            t3: 200,
        });
        clock.push(Sample {
            t0: 0,
            t1: 1_005,
            t2: 1_005,
            t3: 10,
        });
        clock.push(Sample {
            t0: 0,
            t1: 1_050,
            t2: 1_050,
            t3: 100,
        });
        assert_eq!(clock.best().unwrap().rtt(), 10);
        assert_eq!(clock.offset(), 1_000);
    }

    #[test]
    fn it_keeps_only_the_recent_samples() {
        let mut clock = Clock::new();
        for step in 0..SAMPLES as u64 + 4 {
            clock.push(Sample {
                t0: 0,
                t1: 1_000 + step,
                t2: 1_000 + step,
                t3: 2 * step,
            });
        }
        assert_eq!(clock.len(), SAMPLES);
    }

    #[test]
    fn a_sample_index_maps_to_a_play_time() {
        assert_eq!(play_at(1_000, 0, 44_100, 250), 1_250);
        assert_eq!(play_at(1_000, 44_100, 44_100, 250), 2_250);
        assert_eq!(play_at(1_000, 22_050, 44_100, 0), 1_500);
    }

    #[test]
    fn a_tiny_error_is_held() {
        assert!(matches!(correction(0), Nudge::Hold));
        assert!(matches!(correction(1), Nudge::Hold));
        assert!(matches!(correction(-1), Nudge::Hold));
    }

    #[test]
    fn a_small_error_slews_toward_the_host_in_both_directions() {
        assert!(matches!(correction(20), Nudge::Slew(n) if n > 0));
        assert!(matches!(correction(-20), Nudge::Slew(n) if n < 0));
    }

    #[test]
    fn a_large_error_re_anchors_rather_than_slewing() {
        assert!(matches!(correction(500), Nudge::Anchor));
        assert!(matches!(correction(-500), Nudge::Anchor));
    }
}
