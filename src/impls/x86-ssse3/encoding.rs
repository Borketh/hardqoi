use bytemuck::cast_slice;
use core::arch::asm;

use crate::common::{QOIHeader, END_8, MAGIC_QOIF};

use super::hashing::hashes_rgba;

#[path = "encode_context.rs"]
mod encode_context;
use encode_context::EncodeContext;

pub fn encode(raw: &Vec<u8>, meta: QOIHeader, buf: &mut Vec<u8>) -> Result<usize, (usize, usize)> {
    buf.extend(MAGIC_QOIF);
    buf.extend(meta.to_bytes());

    match encode_pixels(raw, buf, meta.image_size()) {
        Ok(n) => {
            buf.extend(END_8);
            Ok(n)
        }
        Err((found, expected)) => Err((found, expected)),
    }
}

#[inline(never)]
fn encode_pixels(
    raw: &Vec<u8>,
    output_buffer: &mut Vec<u8>,
    size: usize,
) -> Result<usize, (usize, usize)> {
    let mut encode_context: EncodeContext = EncodeContext::new(
        cast_slice::<u8, u32>(raw),
        output_buffer,
        hashes_rgba(raw, raw.len() / 4),
    );

    while encode_context.get_pos() < size {
        let pixel = encode_context.get_pixel();
        let pixel_of_same_hash = encode_context.swap_hash();

        if pixel == encode_context.get_previous_pixel() {
            let (max_runs, last_run) = encode_context.find_run_length_at_current_position();
            encode_context.write_run(max_runs, last_run);

            continue;
        }

        if pixel == pixel_of_same_hash {
            encode_context.write_hash_index();
        } else if (pixel & 0xff000000) != (encode_context.get_previous_pixel() & 0xff000000) {
            encode_context.write_rgba();
        } else {
            let mut delta_pixel: u32;

            // pre-biased because that's easier to compare anyway
            let mut delta_pixel_bias2: u32;
            unsafe {
                asm!(
                    "movd {delta_px_xmm}, {pixel:e}",
                    "movd {last_pixel_xmm}, {last_pixel:e}",
                    "psubb {delta_px_xmm}, {last_pixel_xmm}",
                    "movd {delta_px:e}, {delta_px_xmm}",

                    "movd {bias_2_xmm}, {bias:e}",
                    "paddb {bias_2_xmm}, {delta_px_xmm}",
                    "movd {delta_px_b2:e}, {bias_2_xmm}",

                    // input
                    pixel = in(reg) pixel,
                    last_pixel = in(reg) encode_context.get_previous_pixel(),
                    bias = in(reg) 0x00020202,

                    // output ptrs
                    delta_px = out(reg) delta_pixel,
                    delta_px_b2 = out(reg) delta_pixel_bias2,

                    // automatically assigned xmm regs
                    delta_px_xmm = out(xmm_reg) _,
                    last_pixel_xmm = lateout(xmm_reg) _,
                    bias_2_xmm = out(xmm_reg) _,

                    options(preserves_flags, nostack)
                );
            }

            let delta_pixel = bytemuck::cast::<u32, [u8; 4]>(delta_pixel);
            let delta_pixel_bias2 = bytemuck::cast::<u32, [u8; 4]>(delta_pixel_bias2);

            if delta_pixel_bias2[0] < 4u8
                && delta_pixel_bias2[1] < 4u8
                && delta_pixel_bias2[2] < 4u8
            {
                encode_context.write_diff(delta_pixel_bias2);
            } else {
                let [dr, dg, db, _] = delta_pixel;
                let dg_bias32 = dg.wrapping_add(32u8);
                let dr_dg_bias8 = dr.wrapping_sub(dg).wrapping_add(8u8);
                let db_dg_bias8 = db.wrapping_sub(dg).wrapping_add(8u8);

                if dg_bias32 < 64 && dr_dg_bias8 < 16 && db_dg_bias8 < 16 {
                    encode_context.write_luma(dg_bias32, dr_dg_bias8, db_dg_bias8);
                } else {
                    encode_context.write_rgb();
                }
            }
        }

        encode_context.update_pos();
    }

    let last_pos = encode_context.get_pos();
    return if last_pos == size {
        Ok(last_pos)
    } else {
        Err((last_pos, size))
    };
}
