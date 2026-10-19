//! The queue: one list in play order with a cursor on the current track, shuffle that only
//! reorders what is still to come, and repeat.

use serde::{Deserialize, Serialize};

use crate::{Item, QueueSnapshot, Repeat};

/// The tracks to play. It is saved whole to `mobile.sqlite` on every change, so its shape is
/// part of the store: a field added later needs `#[serde(default)]`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Queue {
    /// Every track, in the order it plays.
    entries: Vec<Entry>,
    /// The keys of `entries` in the order they were queued, which turning shuffle off restores.
    queued: Vec<u64>,
    /// The index into `entries` of the current track.
    at: Option<usize>,
    shuffle: bool,
    repeat: Repeat,
    origin: Option<String>,
    /// The key the next entry gets. Keys tell apart two entries of the same track.
    next_key: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    key: u64,
    item: Item,
}

impl Queue {
    /// Replaces everything with `items`, current at `index`. With shuffle on, that track comes
    /// first and the rest follow in a random order.
    pub fn replace(&mut self, items: Vec<Item>, index: usize, origin: Option<String>) {
        self.entries.clear();
        self.queued.clear();
        self.origin = origin;
        let entries = self.wrap(items);
        self.queued = entries.iter().map(|entry| entry.key).collect();
        self.entries = entries;
        self.at = match self.entries.is_empty() {
            true => None,
            false => Some(index.min(self.entries.len() - 1)),
        };
        if self.shuffle
            && let Some(at) = self.at
        {
            let first = self.entries.remove(at);
            self.entries.insert(0, first);
            self.at = Some(0);
            self.shuffle_upcoming();
        }
    }

    /// How many entries the queue holds, the current one included.
    pub fn count(&self) -> usize {
        self.entries.len()
    }

    pub fn current(&self) -> Option<&Item> {
        self.entries.get(self.at?).map(|entry| &entry.item)
    }

    /// The track that follows the current one when it ends, for preloading.
    pub fn upcoming(&self) -> Option<&Item> {
        let at = self.at?;
        let index = match self.repeat {
            Repeat::One => at,
            Repeat::All if at + 1 == self.entries.len() => 0,
            _ => at + 1,
        };
        self.entries.get(index).map(|entry| &entry.item)
    }

    /// Moves to the next track and answers it. `ended` means the current one finished by
    /// itself, which repeat-one answers with the same track. `None` means the queue is done,
    /// and the cursor stays on the last track.
    pub fn advance(&mut self, ended: bool) -> Option<&Item> {
        let at = self.at?;
        let next = match (self.repeat, ended) {
            (Repeat::One, true) => at,
            _ if at + 1 < self.entries.len() => at + 1,
            (Repeat::All | Repeat::One, _) => 0,
            (Repeat::Off, _) => return None,
        };
        self.at = Some(next);
        self.current()
    }

    /// Moves to the track before, wrapping to the last under repeat-all. `None` means there is
    /// none, and the cursor stays.
    pub fn back(&mut self) -> Option<&Item> {
        let at = self.at?;
        let previous = match (at, self.repeat) {
            (0, Repeat::All) => self.entries.len() - 1,
            (0, _) => return None,
            (at, _) => at - 1,
        };
        self.at = Some(previous);
        self.current()
    }

    pub fn set_shuffle(&mut self, on: bool) {
        if on == self.shuffle {
            return;
        }
        self.shuffle = on;
        match on {
            true => self.shuffle_upcoming(),
            false => self.unshuffle(),
        }
    }

    pub fn cycle_repeat(&mut self) {
        self.repeat = match self.repeat {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        };
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    pub fn repeat(&self) -> Repeat {
        self.repeat
    }

    /// Puts `items` right after the current track, or at the start of an empty queue.
    pub fn insert_next(&mut self, items: Vec<Item>) {
        let entries = self.wrap(items);
        let after = self.at.map_or(0, |at| at + 1);
        let keys: Vec<u64> = entries.iter().map(|entry| entry.key).collect();
        let queued_after = self
            .at
            .and_then(|at| self.entries.get(at))
            .and_then(|current| self.queued.iter().position(|key| *key == current.key))
            .map_or(0, |position| position + 1);
        self.entries.splice(after..after, entries);
        self.queued.splice(queued_after..queued_after, keys);
        if self.at.is_none() && !self.entries.is_empty() {
            self.at = Some(0);
        }
    }

    /// Puts `items` at the end.
    pub fn push(&mut self, items: Vec<Item>) {
        let entries = self.wrap(items);
        self.queued.extend(entries.iter().map(|entry| entry.key));
        self.entries.extend(entries);
        if self.at.is_none() && !self.entries.is_empty() {
            self.at = Some(0);
        }
    }

    /// Takes out every track of the provider `slug`, the current one included, and answers
    /// whether anything went. The cursor stays on the current track when it is kept, or else
    /// moves to the next track kept after it, or else to the last one kept before it.
    pub fn drop_provider(&mut self, slug: &str) -> bool {
        let before = self.entries.len();
        let current = self
            .at
            .and_then(|at| self.entries.get(at))
            .map(|entry| entry.key);
        let kept_before = self.at.map(|at| {
            self.entries[..at]
                .iter()
                .filter(|entry| entry.item.provider != slug)
                .count()
        });
        self.entries.retain(|entry| entry.item.provider != slug);
        if self.entries.len() == before {
            return false;
        }
        let kept: std::collections::HashSet<u64> =
            self.entries.iter().map(|entry| entry.key).collect();
        self.queued.retain(|key| kept.contains(key));
        let stayed = current.and_then(|key| self.entries.iter().position(|entry| entry.key == key));
        self.at = match (stayed, kept_before) {
            (Some(at), _) => Some(at),
            (None, Some(next)) if next < self.entries.len() => Some(next),
            (None, Some(previous)) if previous > 0 => Some(previous - 1),
            _ => None,
        };
        true
    }

    /// Takes out the entry at `index` of the play order. The current track cannot be removed,
    /// so asking for it, or for an index past the end, answers false.
    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.entries.len() || Some(index) == self.at {
            return false;
        }
        let entry = self.entries.remove(index);
        self.queued.retain(|key| *key != entry.key);
        if let Some(at) = self.at
            && index < at
        {
            self.at = Some(at - 1);
        }
        true
    }

    pub fn snapshot(&self) -> QueueSnapshot {
        QueueSnapshot {
            items: self
                .entries
                .iter()
                .map(|entry| entry.item.clone())
                .collect(),
            current: self.at.map(|at| at as u32),
            origin: self.origin.clone(),
            shuffle: self.shuffle,
            repeat: self.repeat,
        }
    }

    fn wrap(&mut self, items: Vec<Item>) -> Vec<Entry> {
        items
            .into_iter()
            .map(|item| {
                let key = self.next_key;
                self.next_key += 1;
                Entry { key, item }
            })
            .collect()
    }

    /// Shuffles everything after the current track; what has played stays where it is.
    fn shuffle_upcoming(&mut self) {
        let from = self.at.map_or(0, |at| at + 1);
        if let Some(upcoming) = self.entries.get_mut(from..) {
            fastrand::shuffle(upcoming);
        }
    }

    /// Puts the entries back in the order they were queued, keeping the cursor on the same
    /// track.
    fn unshuffle(&mut self) {
        let current = self
            .at
            .and_then(|at| self.entries.get(at))
            .map(|entry| entry.key);
        let order: std::collections::HashMap<u64, usize> = self
            .queued
            .iter()
            .enumerate()
            .map(|(position, key)| (*key, position))
            .collect();
        self.entries
            .sort_by_key(|entry| order.get(&entry.key).copied().unwrap_or(usize::MAX));
        self.at = current.and_then(|key| self.entries.iter().position(|entry| entry.key == key));
    }
}
