//! YUV 4:2:0 to BGRA, BT.709 limited range (what every verified canvas is tagged), sampled
//! straight to the output size so a 720-wide clip drawn 300 wide costs a 300-wide pass.

use openh264::formats::YUVSource;

/// Converts `yuv` to BGRA at `width` (never wider than the source), keeping the aspect ratio.
/// Odd source sizes are fine: chroma is read at `x / 2`, `y / 2` and clamped to its plane.
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

    let mut out = vec![0u8; out_w * out_h * 4];
    for row in 0..out_h {
        let sy = (row * source_h / out_h).min(source_h - 1);
        let cy = (sy / 2).min(chroma_h - 1);
        for col in 0..out_w {
            let sx = (col * source_w / out_w).min(source_w - 1);
            let cx = (sx / 2).min(chroma_w - 1);
            let y = plane_y[sy * y_stride + sx] as f32;
            let u = plane_u[cy * u_stride + cx] as f32 - 128.0;
            let v = plane_v[cy * v_stride + cx] as f32 - 128.0;
            let luma = 1.164 * (y - 16.0);
            let r = luma + 1.793 * v;
            let g = luma - 0.213 * u - 0.533 * v;
            let b = luma + 2.112 * u;
            let at = (row * out_w + col) * 4;
            out[at] = b.clamp(0.0, 255.0) as u8;
            out[at + 1] = g.clamp(0.0, 255.0) as u8;
            out[at + 2] = r.clamp(0.0, 255.0) as u8;
            out[at + 3] = 255;
        }
    }
    (out, out_w as u32, out_h as u32)
}
