use std::collections::HashMap;
use std::sync::Arc;

use super::wire::{Caps, FromHost, Hit, Line, Pack, Repeat, Who};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Controls {
    pub volume: u8,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub can_next: bool,
    pub can_previous: bool,
    pub favorite: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Seat {
    pub name: String,
    pub native: bool,
    pub device: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lineup {
    pub revision: u64,
    pub total: u32,
    pub at: i32,
    pub rows: Vec<Hit>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Words {
    pub synced: bool,
    pub lines: Vec<Line>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub controls: Controls,
    pub lineup: Lineup,
    pub packs: Arc<Vec<Pack>>,
    pub words: Arc<Words>,
    pub room: Vec<Seat>,
    pub grants: HashMap<String, Caps>,
}

impl Snapshot {
    pub fn can(&self, device: &str) -> Caps {
        self.grants.get(device).copied().unwrap_or_default()
    }
}

pub fn between(was: Option<&Snapshot>, now: &Snapshot, device: &str) -> Vec<FromHost> {
    let mut told = Vec::new();

    let can = now.can(device);
    if was.map(|was| was.can(device)) != Some(can) {
        told.push(FromHost::Grant { can });
    }

    if was.map(|was| was.controls) != Some(now.controls) {
        told.push(FromHost::Controls {
            volume: now.controls.volume,
            shuffle: now.controls.shuffle,
            repeat: now.controls.repeat,
            can_next: now.controls.can_next,
            can_previous: now.controls.can_previous,
            favorite: now.controls.favorite,
        });
    }

    if was.map(|was| &was.lineup) != Some(&now.lineup) {
        told.push(FromHost::Lineup {
            revision: now.lineup.revision,
            total: now.lineup.total,
            at: now.lineup.at,
            rows: now.lineup.rows.clone(),
        });
    }

    let packs = shown(now, can);
    if was.map_or(&[][..], |was| shown(was, was.can(device))) != packs {
        told.push(FromHost::Packs {
            packs: packs.to_vec(),
        });
    }

    if was.map(|was| &was.words) != Some(&now.words) {
        told.push(FromHost::Words {
            synced: now.words.synced,
            lines: now.words.lines.clone(),
        });
    }

    if was.map(|was| &was.room) != Some(&now.room) {
        told.push(FromHost::Room {
            listeners: now.room.iter().map(seen).collect(),
            you: seated(now, device),
        });
    }

    told
}

fn shown(snapshot: &Snapshot, can: Caps) -> &[Pack] {
    match can.browse {
        true => snapshot.packs.as_slice(),
        false => &[],
    }
}

fn seen(seat: &Seat) -> Who {
    Who {
        name: seat.name.clone(),
        native: seat.native,
    }
}

fn seated(now: &Snapshot, device: &str) -> i32 {
    now.room
        .iter()
        .position(|seat| seat.device == device)
        .map(|at| at as i32)
        .unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(control: bool) -> Caps {
        Caps {
            add: true,
            control,
            browse: false,
            favorite: false,
        }
    }

    fn seat(name: &str, device: &str) -> Seat {
        Seat {
            name: name.to_owned(),
            native: false,
            device: device.to_owned(),
        }
    }

    fn room(seats: &[(&str, &str)], grants: &[(&str, Caps)]) -> Snapshot {
        Snapshot {
            room: seats
                .iter()
                .map(|(name, device)| seat(name, device))
                .collect(),
            grants: grants
                .iter()
                .map(|(device, caps)| ((*device).to_owned(), *caps))
                .collect(),
            ..Snapshot::default()
        }
    }

    #[test]
    fn a_first_snapshot_tells_the_listener_everything() {
        let now = room(&[("kitchen", "aa")], &[("aa", caps(true))]);
        let told = between(None, &now, "aa");

        assert!(matches!(told[0], FromHost::Grant { can } if can.control));
        assert!(
            told.iter()
                .any(|told| matches!(told, FromHost::Controls { .. }))
        );
        assert!(
            told.iter()
                .any(|told| matches!(told, FromHost::Lineup { .. }))
        );
        assert!(
            told.iter()
                .any(|told| matches!(told, FromHost::Room { .. }))
        );
    }

    #[test]
    fn nothing_changed_says_nothing() {
        let now = room(&[("kitchen", "aa")], &[("aa", caps(true))]);
        assert!(between(Some(&now.clone()), &now, "aa").is_empty());
    }

    #[test]
    fn a_revoked_grant_reaches_only_the_device_it_was_taken_from() {
        let was = room(
            &[("kitchen", "aa"), ("porch", "bb")],
            &[("aa", caps(true)), ("bb", caps(true))],
        );
        let mut now = was.clone();
        now.grants.insert("bb".to_owned(), caps(false));

        assert!(between(Some(&was), &now, "aa").is_empty());
        let told = between(Some(&was), &now, "bb");
        assert!(matches!(told.as_slice(), [FromHost::Grant { can }] if !can.control));
    }

    #[test]
    fn a_listener_without_a_grant_is_told_it_may_do_nothing() {
        let now = room(&[("kitchen", "aa")], &[]);
        let told = between(None, &now, "aa");
        assert!(matches!(told[0], FromHost::Grant { can } if can == Caps::default()));
    }

    #[test]
    fn the_room_points_each_listener_at_its_own_seat() {
        let now = room(&[("kitchen", "aa"), ("porch", "bb")], &[]);
        let you = |device| {
            between(None, &now, device)
                .into_iter()
                .find_map(|told| match told {
                    FromHost::Room { you, .. } => Some(you),
                    _ => None,
                })
                .unwrap()
        };

        assert_eq!(you("aa"), 0);
        assert_eq!(you("bb"), 1);
        assert_eq!(you("gone"), -1);
    }
}
