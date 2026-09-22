use std::collections::HashMap;

pub const CEILING: u32 = 1_000;

const STEP: u32 = 20;
const HOLD: u64 = 30_000;

pub struct Lead {
    needs: HashMap<String, u32>,
    floor: u32,
    lead: u32,
    eased: Option<u64>,
}

impl Lead {
    pub fn new(floor: u32) -> Self {
        let floor = stepped(floor).min(CEILING);
        Self {
            needs: HashMap::new(),
            floor,
            lead: floor,
            eased: None,
        }
    }

    pub fn note(&mut self, at: &str, need: u32) -> bool {
        let need = stepped(need).min(CEILING);
        self.needs.insert(at.to_owned(), need) != Some(need)
    }

    pub fn forget(&mut self, at: &str) {
        self.needs.remove(at);
    }

    pub fn lead(&self) -> u32 {
        self.lead
    }

    pub fn needs(&self) -> impl Iterator<Item = (&str, u32)> {
        self.needs.iter().map(|(at, need)| (at.as_str(), *need))
    }

    pub fn settle(&mut self, now: u64) -> Option<u32> {
        let want = self
            .needs
            .values()
            .copied()
            .max()
            .unwrap_or(0)
            .clamp(self.floor, CEILING);

        if want > self.lead {
            self.lead = want;
            self.eased = None;
            return Some(want);
        }

        if want == self.lead {
            self.eased = None;
            return None;
        }

        let since = *self.eased.get_or_insert(now);
        if now.saturating_sub(since) < HOLD {
            return None;
        }

        self.lead = want;
        self.eased = None;
        Some(want)
    }
}

fn stepped(value: u32) -> u32 {
    value.div_ceil(STEP).saturating_mul(STEP)
}
