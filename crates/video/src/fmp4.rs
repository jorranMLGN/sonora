//! Fragmented MP4 (DASH) demuxing, read a box at a time. The init segment and the `sidx` index
//! are read once; after that one fragment is read per call, so a reader that blocks until its
//! bytes arrive is only ever asked for what playback needs next.

use std::io::{Cursor, Read, Seek, SeekFrom};
use std::time::Duration;

use anyhow::{Context as _, Result};
use mp4::{BoxHeader, MoofBox, MoovBox, ReadBox as _};

use crate::annexb::Mp4BitstreamConverter;

/// The most an init box (`moov`, `sidx`, `moof`) may claim before the file counts as broken.
const BOX_LIMIT: u64 = 16 << 20;
/// The most one `mdat` may claim: a 5 s fragment of 4K is a few tens of MB.
const MDAT_LIMIT: u64 = 256 << 20;
/// `elst` writes an empty edit as a media time of -1, which `mp4` reads back unsigned.
const EMPTY_EDIT: [u64; 2] = [u32::MAX as u64, u64::MAX];

/// One `sidx` entry: where a fragment lives in the file and when it starts playing.
pub(crate) struct Segment {
    pub offset: u64,
    pub size: u64,
    pub start: Duration,
}

/// One coded picture, still length-prefixed the way MP4 stores it.
pub(crate) struct Sample {
    pub pts: Duration,
    pub duration: Duration,
    pub data: Vec<u8>,
}

/// An opened DASH video stream: its one H.264 track, its timing and its fragment index.
pub(crate) struct Stream<R> {
    reader: R,
    /// Where `reader` is, so reading the next fragment never seeks.
    at: u64,
    track: u32,
    timescale: u32,
    /// The edit list's media time, in `timescale` ticks: raw time `shift` plays at zero.
    shift: u64,
    /// The `trex` defaults: duration, size.
    defaults: (u32, u32),
    /// Where decode time carries on when a fragment has no `tfdt`.
    next_dts: u64,
    converter: Mp4BitstreamConverter,
    pub segments: Vec<Segment>,
}

/// A box header as read from the stream: its type and how long its header and body are.
struct Header {
    name: [u8; 4],
    header: u64,
    body: u64,
}

impl<R: Read + Seek> Stream<R> {
    /// Reads `ftyp`, `moov` and `sidx` from the start of `reader` and stops at the first
    /// fragment. Fails when there is no H.264 track or no `sidx` before the first `moof`.
    pub fn open(mut reader: R) -> Result<Self> {
        let mut at = 0u64;
        let mut moov = None;
        let segments = loop {
            let head = header(&mut reader)?.context("the video ends before its index")?;
            at += head.header;
            match &head.name {
                b"moov" => moov = Some(parse_moov(&read_body(&mut reader, &head, BOX_LIMIT)?)?),
                b"sidx" => {
                    let body = read_body(&mut reader, &head, BOX_LIMIT)?;
                    at += head.body;
                    break parse_sidx(&body, at)?;
                }
                b"moof" | b"mdat" => anyhow::bail!("the video has no sidx before its fragments"),
                _ => skip(&mut reader, head.body)?,
            }
            at += head.body;
        };
        let moov = moov.context("the video has no moov before its index")?;
        let trak = moov
            .traks
            .iter()
            .find(|trak| trak.mdia.minf.stbl.stsd.avc1.is_some())
            .context("the video has no h264 track")?;
        let avcc = &trak
            .mdia
            .minf
            .stbl
            .stsd
            .avc1
            .as_ref()
            .context("no avc1")?
            .avcc;
        let converter = Mp4BitstreamConverter::new(
            avcc.length_size_minus_one + 1,
            avcc.sequence_parameter_sets
                .iter()
                .map(|unit| unit.bytes.clone())
                .collect(),
            avcc.picture_parameter_sets
                .iter()
                .map(|unit| unit.bytes.clone())
                .collect(),
        );
        let track = trak.tkhd.track_id;
        let timescale = trak.mdia.mdhd.timescale;
        anyhow::ensure!(timescale > 0, "the video track has no timescale");
        let shift = trak
            .edts
            .as_ref()
            .and_then(|edts| edts.elst.as_ref())
            .and_then(|elst| {
                elst.entries
                    .iter()
                    .find(|entry| !EMPTY_EDIT.contains(&entry.media_time))
            })
            .map_or(0, |entry| entry.media_time);
        let defaults = moov
            .mvex
            .as_ref()
            .map(|mvex| &mvex.trex)
            .filter(|trex| trex.track_id == track)
            .map_or((0, 0), |trex| {
                (trex.default_sample_duration, trex.default_sample_size)
            });
        let offset = ticks(shift, timescale);
        let segments = segments
            .into_iter()
            .map(|segment| Segment {
                start: segment.start.saturating_sub(offset),
                ..segment
            })
            .collect();
        Ok(Self {
            reader,
            at,
            track,
            timescale,
            shift,
            defaults,
            next_dts: 0,
            converter,
            segments,
        })
    }

    /// The fragment that holds `at`: the last one starting at or before it, or the first.
    pub fn segment_at(&self, at: Duration) -> usize {
        self.segments
            .iter()
            .rposition(|segment| segment.start <= at)
            .unwrap_or(0)
    }

    /// Reads fragment `index` whole, its pictures in decode order. Reads exactly that
    /// fragment's bytes, seeking only when the previous call did not end where it starts.
    pub fn fragment(&mut self, index: usize) -> Result<Vec<Sample>> {
        let segment = self.segments.get(index).context("no such fragment")?;
        let (start, end) = (segment.offset, segment.offset + segment.size);
        if self.at != start {
            self.reader
                .seek(SeekFrom::Start(start))
                .context("cannot seek to the fragment")?;
            self.at = start;
        }
        let mut moof = None;
        let mut samples = Vec::new();
        while self.at < end {
            let head = header(&mut self.reader)?.context("the fragment ends early")?;
            let begins = self.at;
            self.at += head.header;
            match &head.name {
                b"moof" => {
                    let body = read_body(&mut self.reader, &head, BOX_LIMIT)?;
                    moof = Some((begins, parse_moof(&body)?));
                }
                b"mdat" => {
                    let body = read_body(&mut self.reader, &head, MDAT_LIMIT)?;
                    if let Some((moof_at, moof)) = moof.take() {
                        self.samples(&moof, moof_at, self.at, &body, &mut samples)?;
                    }
                }
                _ => skip(&mut self.reader, head.body)?,
            }
            self.at += head.body;
        }
        Ok(samples)
    }

    /// A fresh Annex-B converter for this stream, for the start of a decode.
    pub fn converter(&self) -> Mp4BitstreamConverter {
        self.converter.clone()
    }

    /// Cuts the pictures of our track out of one `moof` and the `mdat` after it.
    fn samples(
        &mut self,
        moof: &MoofBox,
        moof_at: u64,
        mdat_at: u64,
        mdat: &[u8],
        out: &mut Vec<Sample>,
    ) -> Result<()> {
        for traf in moof
            .trafs
            .iter()
            .filter(|traf| traf.tfhd.track_id == self.track)
        {
            let Some(trun) = &traf.trun else {
                continue;
            };
            let base = traf.tfhd.base_data_offset.unwrap_or(moof_at);
            let mut offset = base
                .checked_add_signed(i64::from(trun.data_offset.unwrap_or(0)))
                .context("a sample lies before the file")?;
            let mut dts = traf
                .tfdt
                .as_ref()
                .map_or(self.next_dts, |tfdt| tfdt.base_media_decode_time);
            for at in 0..trun.sample_count as usize {
                let size = trun
                    .sample_sizes
                    .get(at)
                    .copied()
                    .or(traf.tfhd.default_sample_size)
                    .unwrap_or(self.defaults.1);
                let duration = trun
                    .sample_durations
                    .get(at)
                    .copied()
                    .or(traf.tfhd.default_sample_duration)
                    .unwrap_or(self.defaults.0);
                let composition = trun
                    .sample_cts
                    .get(at)
                    .map_or(0, |&cts| match trun.version {
                        0 => i64::from(cts),
                        _ => i64::from(cts as i32),
                    });
                let from = offset
                    .checked_sub(mdat_at)
                    .context("a sample lies outside its mdat")? as usize;
                let data = mdat
                    .get(from..from.saturating_add(size as usize))
                    .context("a sample lies outside its mdat")?;
                let raw = (dts as i64)
                    .saturating_add(composition)
                    .saturating_sub(self.shift as i64)
                    .max(0) as u64;
                out.push(Sample {
                    pts: ticks(raw, self.timescale),
                    duration: ticks(u64::from(duration), self.timescale),
                    data: data.to_vec(),
                });
                dts += u64::from(duration);
                offset += u64::from(size);
            }
            self.next_dts = dts;
        }
        Ok(())
    }

    #[cfg(test)]
    fn position(&mut self) -> u64 {
        self.reader.stream_position().unwrap_or(0)
    }
}

/// Reads one box header, or `None` at a clean end of the stream.
fn header(reader: &mut impl Read) -> Result<Option<Header>> {
    let mut head = [0u8; 8];
    match reader.read_exact(&mut head) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error).context("cannot read a box header"),
    }
    let size = u64::from(u32::from_be_bytes([head[0], head[1], head[2], head[3]]));
    let name = [head[4], head[5], head[6], head[7]];
    let (size, header) = match size {
        0 => anyhow::bail!("a box runs to the end of the file"),
        1 => {
            let mut large = [0u8; 8];
            reader
                .read_exact(&mut large)
                .context("cannot read a box size")?;
            (u64::from_be_bytes(large), 16)
        }
        size => (size, 8),
    };
    let body = size
        .checked_sub(header)
        .context("a box is shorter than its header")?;
    Ok(Some(Header { name, header, body }))
}

fn read_body(reader: &mut impl Read, head: &Header, limit: u64) -> Result<Vec<u8>> {
    anyhow::ensure!(head.body <= limit, "a box claims {} bytes", head.body);
    let mut body = vec![0u8; head.body as usize];
    reader.read_exact(&mut body).context("cannot read a box")?;
    Ok(body)
}

fn skip(reader: &mut impl Seek, length: u64) -> Result<()> {
    let length = i64::try_from(length).context("a box is too long")?;
    reader
        .seek(SeekFrom::Current(length))
        .context("cannot skip a box")?;
    Ok(())
}

/// `mp4`'s box readers want their own header in front of the body and a seekable reader.
fn framed(name: &[u8; 4], body: &[u8]) -> Result<(Cursor<Vec<u8>>, u64)> {
    let size = u32::try_from(body.len() + 8).context("a box is too long")?;
    let mut bytes = Vec::with_capacity(body.len() + 8);
    bytes.extend(size.to_be_bytes());
    bytes.extend(name);
    bytes.extend(body);
    let mut cursor = Cursor::new(bytes);
    let head = BoxHeader::read(&mut cursor).context("cannot read a box header")?;
    Ok((cursor, head.size))
}

fn parse_moov(body: &[u8]) -> Result<MoovBox> {
    let (mut cursor, size) = framed(b"moov", body)?;
    MoovBox::read_box(&mut cursor, size).context("cannot read the moov")
}

fn parse_moof(body: &[u8]) -> Result<MoofBox> {
    let (mut cursor, size) = framed(b"moof", body)?;
    MoofBox::read_box(&mut cursor, size).context("cannot read a moof")
}

/// Reads a `sidx` body whose box ends at file offset `end` into its segments.
fn parse_sidx(body: &[u8], end: u64) -> Result<Vec<Segment>> {
    let version = *body.first().context("the sidx is empty")?;
    let timescale = be(body, 8, 4)? as u32;
    anyhow::ensure!(timescale > 0, "the sidx has no timescale");
    let (mut time, first, mut at) = match version {
        0 => (be(body, 12, 4)?, be(body, 16, 4)?, 20),
        _ => (be(body, 12, 8)?, be(body, 20, 8)?, 28),
    };
    let count = be(body, at + 2, 2)?;
    at += 4;
    let mut offset = end + first;
    let mut segments = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let reference = be(body, at, 4)?;
        let duration = be(body, at + 4, 4)?;
        anyhow::ensure!(reference >> 31 == 0, "the sidx points at another sidx");
        let size = reference & 0x7fff_ffff;
        segments.push(Segment {
            offset,
            size,
            start: ticks(time, timescale),
        });
        offset += size;
        time += duration;
        at += 12;
    }
    anyhow::ensure!(!segments.is_empty(), "the sidx lists no fragments");
    Ok(segments)
}

/// Reads a big-endian number `width` bytes long at `at`.
fn be(bytes: &[u8], at: usize, width: usize) -> Result<u64> {
    let field = bytes.get(at..at + width).context("the sidx is cut short")?;
    Ok(field
        .iter()
        .fold(0, |sum, &byte| (sum << 8) | u64::from(byte)))
}

fn ticks(ticks: u64, timescale: u32) -> Duration {
    Duration::from_nanos((u128::from(ticks) * 1_000_000_000 / u128::from(timescale)) as u64)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../tests/fixtures/clip-dash.mp4");

    #[test]
    fn the_fixture_indexes_four_fragments() {
        let mut stream = Stream::open(Cursor::new(FIXTURE.to_vec())).unwrap();
        assert_eq!(stream.segments.len(), 4);
        assert_eq!(stream.segments[0].start, Duration::ZERO);
        // opening reads the init segment and the index, and nothing of the first fragment
        let first = stream.segments[0].offset;
        assert_eq!(stream.position(), first);
        assert_eq!(stream.segment_at(Duration::from_millis(3300)), 3);

        let samples = stream.fragment(1).unwrap();
        assert_eq!(samples.len(), 25);
        let earliest = samples.iter().map(|sample| sample.pts).min().unwrap();
        assert_eq!(earliest, Duration::from_millis(1080));
        assert!(samples.iter().all(|sample| !sample.data.is_empty()));
    }
}
