//! YUV 4:2:0 to BGRA, BT.709 limited range (what every verified canvas is tagged), sampled
//! straight to the output size so a 720-wide clip drawn 300 wide costs a 300-wide pass.
//!
//! The math is fixed point and runs a row at a time: a 1080p picture converts in a few
//! milliseconds, where a float pass per pixel took 15 ms optimised and 70 ms in a debug build.
//! OpenH264's own SIMD conversion is faster still, but it is BT.601 and would shift the colours.

use openh264::formats::YUVSource;

/// Bits of fraction in the fixed-point coefficients.
const SHIFT: u32 = 14;
/// Half of one, added before the shift so the result rounds.
const HALF: i32 = 1 << (SHIFT - 1);
/// BT.709 limited range, times `1 << SHIFT`: luma gain, then V into red, U and V into green,
/// and U into blue.
const Y_GAIN: i32 = 19_071; // 1.164
const RV: i32 = 29_377; // 1.793
const GU: i32 = 3_490; // 0.213
const GV: i32 = 8_733; // 0.533
const BU: i32 = 34_603; // 2.112

/// Converts `yuv` to BGRA at `width` (never wider than the source), keeping the aspect ratio.
/// Odd source sizes are fine: chroma is read at `x / 2`, `y / 2` and clamped to its plane.
/// A narrower output samples the nearest source pixel.
pub(crate) fn bgra(yuv: &impl YUVSource, width: u32) -> (Vec<u8>, u32, u32) {
    let (source_w, source_h) = yuv.dimensions();
    let out_w = (width as usize).clamp(1, source_w);
    let out_h = ((source_h * out_w) as f32 / source_w as f32)
        .round()
        .max(1.0) as usize;
    let (y_stride, u_stride, v_stride) = yuv.strides();
    let (plane_y, plane_u, plane_v) = (yuv.y(), yuv.u(), yuv.v());
    let chroma_w = source_w.div_ceil(2);
    let chroma_h = source_h.div_ceil(2);
    // the source column of every output column, and its chroma column
    let columns: Vec<usize> = (0..out_w)
        .map(|col| (col * source_w / out_w).min(source_w - 1))
        .collect();
    let chroma_columns: Vec<usize> = columns
        .iter()
        .map(|sx| (sx / 2).min(chroma_w - 1))
        .collect();
    let full = out_w == source_w;

    // one row of luma and chroma at the output width, refilled as the source row changes
    let mut luma = vec![0u8; out_w];
    let (mut u, mut v) = (vec![0u8; out_w], vec![0u8; out_w]);
    let mut chroma_row = None;
    let mut out = vec![0u8; out_w * out_h * 4];
    for (row, line) in out.chunks_exact_mut(out_w * 4).enumerate() {
        let sy = (row * source_h / out_h).min(source_h - 1);
        let cy = (sy / 2).min(chroma_h - 1);
        let source = &plane_y[sy * y_stride..][..source_w];
        let luma = match full {
            true => source,
            false => {
                gather(source, &columns, &mut luma);
                &luma
            }
        };
        if chroma_row != Some(cy) {
            chroma_row = Some(cy);
            gather(
                &plane_u[cy * u_stride..][..chroma_w],
                &chroma_columns,
                &mut u,
            );
            gather(
                &plane_v[cy * v_stride..][..chroma_w],
                &chroma_columns,
                &mut v,
            );
        }
        convert_row(luma, &u, &v, line);
    }
    (out, out_w as u32, out_h as u32)
}

/// Fills `into` with `from` sampled at `columns`.
fn gather(from: &[u8], columns: &[usize], into: &mut [u8]) {
    for (to, &column) in into.iter_mut().zip(columns) {
        *to = from[column];
    }
}

/// Converts one row whose luma and chroma are already at the output width, with AVX2 when the
/// processor has it.
fn convert_row(luma: &[u8], u: &[u8], v: &[u8], line: &mut [u8]) {
    #[cfg(target_arch = "x86_64")]
    if std::arch::is_x86_feature_detected!("avx2") {
        // SAFETY: the processor was just found to support AVX2
        unsafe { convert_row_avx2(luma, u, v, line) };
        return;
    }
    convert_row_portable(luma, u, v, line);
}

/// `convert_row_portable` compiled for AVX2: eight pixels a step instead of four.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
fn convert_row_avx2(luma: &[u8], u: &[u8], v: &[u8], line: &mut [u8]) {
    convert_row_portable(luma, u, v, line);
}

/// The row conversion, written as one straight loop over equal-length slices so the compiler
/// turns it into vector code. Debug assertions stop it vectorising, which is why the dev
/// profile builds this crate without them.
#[inline(always)]
fn convert_row_portable(luma: &[u8], u: &[u8], v: &[u8], line: &mut [u8]) {
    let pixels = line
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(luma)
        .zip(u.iter().zip(v));
    for ((pixel, &y), (&u, &v)) in pixels {
        let (y, u, v) = (i32::from(y) - 16, i32::from(u) - 128, i32::from(v) - 128);
        let luma = Y_GAIN * y + HALF;
        let b = channel(luma + BU * u);
        let g = channel(luma - GU * u - GV * v);
        let r = channel(luma + RV * v);
        *pixel = (b | g << 8 | r << 16 | 0xff00_0000).to_le_bytes();
    }
}

/// One fixed-point channel, rounded and clamped to a byte.
fn channel(value: i32) -> u32 {
    (value >> SHIFT).clamp(0, 255) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4:2:0 picture of any size, its planes padded past the width like a decoder's.
    struct Planes {
        size: (usize, usize),
        strides: (usize, usize, usize),
        y: Vec<u8>,
        u: Vec<u8>,
        v: Vec<u8>,
    }

    impl YUVSource for Planes {
        fn dimensions(&self) -> (usize, usize) {
            self.size
        }
        fn strides(&self) -> (usize, usize, usize) {
            self.strides
        }
        fn y(&self) -> &[u8] {
            &self.y
        }
        fn u(&self) -> &[u8] {
            &self.u
        }
        fn v(&self) -> &[u8] {
            &self.v
        }
    }

    /// A `width` by `height` picture whose every sample walks the whole 0..=255 range.
    fn planes(width: usize, height: usize) -> Planes {
        let (chroma_w, chroma_h) = (width.div_ceil(2), height.div_ceil(2));
        let strides = (width + 32, chroma_w + 16, chroma_w + 16);
        let mut seed = 0x2545_f491_u32;
        let mut noise = |len: usize| {
            (0..len)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    seed as u8
                })
                .collect::<Vec<u8>>()
        };
        Planes {
            size: (width, height),
            strides,
            y: noise(strides.0 * height),
            u: noise(strides.1 * chroma_h),
            v: noise(strides.2 * chroma_h),
        }
    }

    /// The float conversion this module used before, kept as the reference.
    fn reference(yuv: &impl YUVSource, width: u32) -> (Vec<u8>, u32, u32) {
        let (source_w, source_h) = yuv.dimensions();
        let out_w = (width as usize).clamp(1, source_w);
        let out_h = ((source_h * out_w) as f32 / source_w as f32)
            .round()
            .max(1.0) as usize;
        let (y_stride, u_stride, v_stride) = yuv.strides();
        let (chroma_w, chroma_h) = (source_w.div_ceil(2), source_h.div_ceil(2));
        let mut out = vec![0u8; out_w * out_h * 4];
        for row in 0..out_h {
            let sy = (row * source_h / out_h).min(source_h - 1);
            let cy = (sy / 2).min(chroma_h - 1);
            for col in 0..out_w {
                let sx = (col * source_w / out_w).min(source_w - 1);
                let cx = (sx / 2).min(chroma_w - 1);
                let y = yuv.y()[sy * y_stride + sx] as f32;
                let u = yuv.u()[cy * u_stride + cx] as f32 - 128.0;
                let v = yuv.v()[cy * v_stride + cx] as f32 - 128.0;
                let luma = 1.164 * (y - 16.0);
                let at = (row * out_w + col) * 4;
                out[at] = (luma + 2.112 * u).clamp(0.0, 255.0) as u8;
                out[at + 1] = (luma - 0.213 * u - 0.533 * v).clamp(0.0, 255.0) as u8;
                out[at + 2] = (luma + 1.793 * v).clamp(0.0, 255.0) as u8;
                out[at + 3] = 255;
            }
        }
        (out, out_w as u32, out_h as u32)
    }

    /// The largest difference of any channel between the fast and the reference conversion.
    fn worst(width: usize, height: usize, out: u32) -> u8 {
        let yuv = planes(width, height);
        let (fast, fast_w, fast_h) = bgra(&yuv, out);
        let (slow, slow_w, slow_h) = reference(&yuv, out);
        assert_eq!((fast_w, fast_h), (slow_w, slow_h));
        fast.iter()
            .zip(&slow)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap()
    }

    #[test]
    fn full_width_matches_the_float_conversion() {
        assert!(worst(256, 144, 256) <= 1);
    }

    #[test]
    fn odd_sizes_match_the_float_conversion() {
        assert!(worst(255, 143, 255) <= 1);
        assert!(worst(255, 143, 100) <= 1);
    }

    #[test]
    fn a_narrower_output_matches_the_float_conversion() {
        assert!(worst(1920, 1080, 700) <= 1);
        assert!(worst(1920, 1080, 1919) <= 1);
    }
}
