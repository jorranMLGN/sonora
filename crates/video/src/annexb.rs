//! MP4 keeps H.264 as length-prefixed NAL units and its SPS/PPS in the avcC box; openh264 wants
//! Annex-B start codes. Adapted from openh264-rs's own example (BSD-2-Clause).

use anyhow::anyhow;
use mp4::Mp4Track;

/// Network abstraction layer type for H264 pocket we might find.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NalType {
    Unspecified = 0,
    Slice = 1,
    Dpa = 2,
    Dpb = 3,
    Dpc = 4,
    IdrSlice = 5,
    Sei = 6,
    Sps = 7,
    Pps = 8,
    Aud = 9,
    EndSequence = 10,
    EndStream = 11,
    FillerData = 12,
    SpsExt = 13,
    Prefix = 14,
    SubSps = 15,
    Dps = 16,
    Reserved17 = 17,
    Reserved18 = 18,
    AuxiliarySlice = 19,
    ExtenSlice = 20,
    DepthExtenSlice = 21,
    Reserved22 = 22,
    Reserved23 = 23,
    Unspecified24 = 24,
    Unspecified25 = 25,
    Unspecified26 = 26,
    Unspecified27 = 27,
    Unspecified28 = 28,
    Unspecified29 = 29,
    Unspecified30 = 30,
    Unspecified31 = 31,
}

#[allow(clippy::fallible_impl_from)]
impl From<u8> for NalType {
    /// Reads NAL from header byte.
    fn from(value: u8) -> Self {
        use NalType::{
            Aud, AuxiliarySlice, DepthExtenSlice, Dpa, Dpb, Dpc, Dps, EndSequence, EndStream,
            ExtenSlice, FillerData, IdrSlice, Pps, Prefix, Reserved17, Reserved18, Reserved22,
            Reserved23, Sei, Slice, Sps, SpsExt, SubSps, Unspecified, Unspecified24, Unspecified25,
            Unspecified26, Unspecified27, Unspecified28, Unspecified29, Unspecified30,
            Unspecified31,
        };

        match value {
            0 => Unspecified,
            1 => Slice,
            2 => Dpa,
            3 => Dpb,
            4 => Dpc,
            5 => IdrSlice,
            6 => Sei,
            7 => Sps,
            8 => Pps,
            9 => Aud,
            10 => EndSequence,
            11 => EndStream,
            12 => FillerData,
            13 => SpsExt,
            14 => Prefix,
            15 => SubSps,
            16 => Dps,
            17 => Reserved17,
            18 => Reserved18,
            19 => AuxiliarySlice,
            20 => ExtenSlice,
            21 => DepthExtenSlice,
            22 => Reserved22,
            23 => Reserved23,
            24 => Unspecified24,
            25 => Unspecified25,
            26 => Unspecified26,
            27 => Unspecified27,
            28 => Unspecified28,
            29 => Unspecified29,
            30 => Unspecified30,
            31 => Unspecified31,
            _ => panic!("Invalid NAL type"),
        }
    }
}

/// A NAL unit in a bitstream.
struct NalUnit<'a> {
    nal_type: NalType,
    bytes: &'a [u8],
}

impl<'a> NalUnit<'a> {
    /// Reads a NAL unit from a slice of bytes in MP4, returning the unit, and the remaining stream
    /// after that slice. `None` for a zero length or one that runs past the end: the file comes
    /// from a CDN, so a malformed length must end the packet, never index out of bounds.
    fn from_stream(mut stream: &'a [u8], length_size: u8) -> Option<(Self, &'a [u8])> {
        let mut nal_size = 0usize;

        // Construct nal_size from first bytes in MP4 stream.
        for _ in 0..length_size {
            let (&byte, rest) = stream.split_first()?;
            nal_size = (nal_size << 8) | usize::from(byte);
            stream = rest;
        }

        if nal_size == 0 || nal_size > stream.len() {
            return None;
        }

        let (packet, rest) = stream.split_at(nal_size);
        let nal_type = NalType::from(packet[0] & 0x1F);
        let unit = NalUnit {
            nal_type,
            bytes: packet,
        };

        Some((unit, rest))
    }

    #[allow(unused)]
    const fn nal_type(&self) -> NalType {
        self.nal_type
    }

    #[allow(unused)]
    const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

/// Converter from NAL units from the MP4 to the Annex B format expected by openh264.
///
/// It also inserts SPS and PPS units from the MP4 header into the stream.
/// They are also required for Annex B format to be decodable, but are not present in the MP4 bitstream,
/// as they are stored in the headers.
#[derive(Clone)]
pub(crate) struct Mp4BitstreamConverter {
    length_size: u8,
    sps: Vec<Vec<u8>>,
    pps: Vec<Vec<u8>>,
    new_idr: bool,
    sps_seen: bool,
    pps_seen: bool,
}

impl Mp4BitstreamConverter {
    /// Create a new converter for the given track.
    ///
    /// The track must contain an AVC1 configuration.
    pub(crate) fn for_mp4_track(track: &Mp4Track) -> Result<Self, anyhow::Error> {
        let avcc_config = &track
            .trak
            .mdia
            .minf
            .stbl
            .stsd
            .avc1
            .as_ref()
            .ok_or_else(|| anyhow!("Track does not contain AVC1 config"))?
            .avcc;

        Ok(Self::new(
            avcc_config.length_size_minus_one + 1,
            avcc_config
                .sequence_parameter_sets
                .iter()
                .map(|v| v.bytes.clone())
                .collect(),
            avcc_config
                .picture_parameter_sets
                .iter()
                .map(|v| v.bytes.clone())
                .collect(),
        ))
    }

    /// A converter from an avcC's NAL length size and its SPS and PPS units, for a stream that
    /// is not read through `mp4::Mp4Reader`.
    pub(crate) fn new(length_size: u8, sps: Vec<Vec<u8>>, pps: Vec<Vec<u8>>) -> Self {
        Self {
            length_size,
            sps,
            pps,
            new_idr: true,
            sps_seen: false,
            pps_seen: false,
        }
    }

    /// Convert a single packet from the MP4 format to the Annex B format.
    ///
    /// It clears the `out` vector and appends the converted packet to it.
    pub(crate) fn convert_packet(&mut self, packet: &[u8], out: &mut Vec<u8>) {
        let mut stream = packet;
        out.clear();

        while !stream.is_empty() {
            // upstream's example looped here forever on a zero length; drop the rest instead
            let Some((unit, remaining_stream)) = NalUnit::from_stream(stream, self.length_size)
            else {
                break;
            };

            stream = remaining_stream;

            match unit.nal_type {
                NalType::Sps => self.sps_seen = true,
                NalType::Pps => self.pps_seen = true,
                NalType::IdrSlice => {
                    // If this is a new IDR picture following an IDR picture, reset the idr flag.
                    // Just check first_mb_in_slice to be 1
                    if !self.new_idr && unit.bytes.get(1).is_some_and(|byte| byte & 0x80 != 0) {
                        self.new_idr = true;
                    }
                    // insert SPS & PPS NAL units if they were not seen
                    if self.new_idr && !self.sps_seen && !self.pps_seen {
                        self.new_idr = false;
                        for sps in &self.sps {
                            out.extend([0, 0, 1]);
                            out.extend(sps);
                        }
                        for pps in &self.pps {
                            out.extend([0, 0, 1]);
                            out.extend(pps);
                        }
                    }
                    // insert only PPS if SPS was seen
                    if self.new_idr && self.sps_seen && !self.pps_seen {
                        for pps in &self.pps {
                            out.extend([0, 0, 1]);
                            out.extend(pps);
                        }
                    }
                }
                _ => {}
            }

            out.extend([0, 0, 1]);
            out.extend(unit.bytes);

            if !self.new_idr && unit.nal_type == NalType::Slice {
                self.new_idr = true;
                self.sps_seen = false;
                self.pps_seen = false;
            }
        }
    }
}
