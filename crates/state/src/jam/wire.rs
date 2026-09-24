use anyhow::Context as _;
use anyhow::{Result, bail};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub const PROTOCOL: u32 = 5;
pub const MAGIC: [u8; 4] = *b"SNJ1";
pub const HEADER: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub seq: u32,
    pub first_sample: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    Pcm16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Format {
    pub rate: u32,
    pub channels: u16,
    pub codec: Codec,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Caps {
    pub add: bool,
    pub control: bool,
    pub browse: bool,
    pub favorite: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cap {
    Add,
    Control,
    Browse,
    Favorite,
}

impl Cap {
    pub const ALL: [Cap; 4] = [Cap::Add, Cap::Control, Cap::Browse, Cap::Favorite];

    pub fn key(self) -> &'static str {
        match self {
            Cap::Add => "jam-cap-add",
            Cap::Control => "jam-cap-control",
            Cap::Browse => "jam-cap-browse",
            Cap::Favorite => "jam-cap-favorite",
        }
    }

    pub fn of(self, caps: &Caps) -> bool {
        match self {
            Cap::Add => caps.add,
            Cap::Control => caps.control,
            Cap::Browse => caps.browse,
            Cap::Favorite => caps.favorite,
        }
    }

    pub fn set(self, caps: &mut Caps, on: bool) {
        match self {
            Cap::Add => caps.add = on,
            Cap::Control => caps.control = on,
            Cap::Browse => caps.browse = on,
            Cap::Favorite => caps.favorite = on,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "act")]
pub enum Act {
    Play,
    Pause,
    Next,
    Previous,
    Seek { ms: u64 },
    Volume { level: u8 },
    Shuffle { on: bool },
    Repeat,
    Jump { index: i32 },
    Drop { index: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkKind {
    Quiet,
    Cut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Refusal {
    Protocol,
    Code,
    Codec,
    Full,
    Closed,
    Kicked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hit {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub cover: Option<String>,
    pub duration_ms: u64,
    pub provider: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Denial {
    Closed,
    Unknown,
    Busy,
    Forbidden,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Who {
    pub name: String,
    pub native: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pack {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub cover: Option<String>,
    pub tracks: u32,
    pub provider: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub at: u64,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Farewell {
    HostLeft,
    Kicked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum FromReceiver {
    Hello {
        protocol: u32,
        name: String,
        native: bool,
        code: String,
        device: String,
        accepts: Vec<Codec>,
    },
    Ping {
        t0: u64,
        need: u32,
    },
    Find {
        query: String,
    },
    Add {
        id: String,
    },
    Command {
        act: Act,
    },
    Open {
        pack: String,
    },
    Enqueue {
        pack: String,
        next: bool,
    },
    Favorite {
        id: String,
        on: bool,
    },
    Bye,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum FromHost {
    Welcome {
        protocol: u32,
        room: String,
        format: Format,
        lead_ms: u32,
        origin: u64,
    },
    Refused {
        reason: Refusal,
    },
    Grant {
        can: Caps,
    },
    Controls {
        volume: u8,
        shuffle: bool,
        repeat: Repeat,
        can_next: bool,
        can_previous: bool,
        favorite: bool,
    },
    Room {
        listeners: Vec<Who>,
        you: i32,
    },
    Lineup {
        revision: u64,
        total: u32,
        at: i32,
        rows: Vec<Hit>,
    },
    Packs {
        packs: Vec<Pack>,
    },
    Opened {
        pack: String,
        total: u32,
        rows: Vec<Hit>,
    },
    Words {
        synced: bool,
        lines: Vec<Line>,
    },
    Mark {
        mark: MarkKind,
        at: u64,
        origin: u64,
    },
    Lead {
        lead_ms: u32,
    },
    Now {
        title: String,
        artist: String,
        album: String,
        cover: Option<String>,
        duration_ms: u64,
        provider: String,
    },
    Transport {
        playing: bool,
        position_ms: u64,
    },
    Pong {
        t0: u64,
        t1: u64,
        t2: u64,
    },
    Found {
        query: String,
        hits: Vec<Hit>,
    },
    Added {
        title: String,
    },
    Denied {
        reason: Denial,
    },
    Ended {
        reason: Farewell,
    },
}

impl Header {
    pub fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&self.seq.to_le_bytes());
        out.extend_from_slice(&self.first_sample.to_le_bytes());
    }

    pub fn read(frame: &[u8]) -> Result<Self> {
        if frame.len() < HEADER {
            bail!("cannot read a frame shorter than the header");
        }
        if frame[..MAGIC.len()] != MAGIC {
            bail!("cannot read a frame without the jam magic");
        }

        Ok(Self {
            seq: u32::from_le_bytes(frame[4..8].try_into()?),
            first_sample: u64::from_le_bytes(frame[8..HEADER].try_into()?),
        })
    }
}

pub fn encode<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).context("cannot encode a jam message")
}

pub fn decode<T: DeserializeOwned>(line: &str) -> Result<T> {
    serde_json::from_str(line).context("cannot decode a jam message")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_round_trips() {
        let header = Header {
            seq: 4_294_967_000,
            first_sample: 1 << 40,
        };
        let mut frame = Vec::new();
        header.write(&mut frame);
        assert_eq!(frame.len(), HEADER);
        assert_eq!(Header::read(&frame).unwrap(), header);
    }

    #[test]
    fn a_frame_without_the_magic_is_refused() {
        let mut frame = vec![0u8; HEADER];
        frame[0..4].copy_from_slice(b"XXXX");
        assert!(Header::read(&frame).is_err());
    }

    #[test]
    fn a_short_frame_is_refused_rather_than_panicking() {
        for len in 0..HEADER {
            assert!(Header::read(&vec![0u8; len]).is_err(), "{len}");
        }
    }

    #[test]
    fn a_hello_round_trips() {
        let sent = FromReceiver::Hello {
            protocol: PROTOCOL,
            name: "kitchen".into(),
            native: true,
            code: "204813".into(),
            device: "8f14e45fceea167a".into(),
            accepts: vec![Codec::Pcm16],
        };
        assert_eq!(
            decode::<FromReceiver>(&encode(&sent).unwrap()).unwrap(),
            sent
        );
    }

    #[test]
    fn a_codec_nobody_knows_is_refused() {
        assert!(decode::<Codec>(r#""opus""#).is_err());
        assert_eq!(decode::<Codec>(r#""pcm16""#).unwrap(), Codec::Pcm16);
    }

    #[test]
    fn a_frame_is_always_one_line() {
        let sent = FromHost::Now {
            title: "a\nname\twith\r\nbreaks".into(),
            artist: "x".into(),
            album: "y".into(),
            cover: None,
            duration_ms: 214_000,
            provider: "spotify".into(),
        };
        let line = encode(&sent).unwrap();
        assert!(!line.contains('\n'));
        assert_eq!(decode::<FromHost>(&line).unwrap(), sent);
    }

    #[test]
    fn every_mark_round_trips() {
        for mark in [MarkKind::Quiet, MarkKind::Cut] {
            let sent = FromHost::Mark {
                mark,
                at: 7_000_000,
                origin: 1_764_000_000_000,
            };
            assert_eq!(decode::<FromHost>(&encode(&sent).unwrap()).unwrap(), sent);
        }
    }

    #[test]
    fn an_unknown_variant_is_refused() {
        assert!(decode::<FromHost>(r#"{"kind":"Nonsense"}"#).is_err());
        assert!(decode::<FromReceiver>("not json").is_err());
        assert!(decode::<FromReceiver>("").is_err());
    }
}
