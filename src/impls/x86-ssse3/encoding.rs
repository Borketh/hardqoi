use bytemuck::cast_slice;
use core::arch::asm;

use crate::common::{
    QOIHeader, END_8, MAGIC_QOIF, QOI_OP_DIFF, QOI_OP_INDEX, QOI_OP_LUMA, QOI_OP_RGB, QOI_OP_RGBA,
    QOI_OP_RUN,
};

use super::hashing::hashes_rgba;

pub fn encode(raw: &Vec<u8>, meta: QOIHeader, buf: &mut Vec<u8>) -> Result<usize, (usize, usize)> {
    buf.extend(MAGIC_QOIF);
    buf.extend(meta.to_bytes());

    match encode_pixels(raw, buf) {
        Ok(n) => {
            buf.extend(END_8);
            Ok(n)
        }
        Err((found, expected)) => Err((found, expected)),
    }
}

fn encode_pixels(raw: &Vec<u8>, output_buffer: &mut Vec<u8>) -> Result<usize, (usize, usize)> {
    let pixels: &[[u8; 4]] = cast_slice::<u8, [u8; 4]>(raw);
    let hashes: Vec<u8> = hashes_rgba(raw, pixels.len());

    let mut prev_pixel: [u8; 4] = [0, 0, 0, 255];
    let mut hash_indexed_array: [[u8; 4]; 64] = [[0u8; 4]; 64];
    let pixel_run_counter: &mut u8 = &mut 0u8;
    let mut px_written = 0usize;

    // dbg!(&hashes);

    for (i, &pixel) in pixels.iter().enumerate() {
        let pixel_of_same_hash = hash_swap(&mut hash_indexed_array, pixel, hashes[i]);

        if pixel == prev_pixel {
            *pixel_run_counter += 1u8;
            if *pixel_run_counter == 62u8 {
                // we have to cut off early for overflow
                px_written += *pixel_run_counter as usize;
                write_run(output_buffer, pixel_run_counter);
            }
            continue;
        } else if *pixel_run_counter > 0u8 {
            // end the previous run and keep going down the comparisons
            px_written += *pixel_run_counter as usize;
            write_run(output_buffer, pixel_run_counter);
        }

        if pixel == pixel_of_same_hash {
            px_written += 1;
            write_hash_index(output_buffer, hashes[i]);
        } else if pixel[3] != prev_pixel[3] {
            px_written += 1;
            write_rgba(output_buffer, pixel);
        } else {
            let mut delta_pixel: u32;

            // pre-biased because that's easier to compare anyway
            let mut delta_pixel_bias2: u32;
            unsafe {
                asm!(
                    "movd {delta_px_xmm}, [{pixel_ptr}]",
                    "movd {last_pixel_xmm}, [{last_pixel_ptr}]",
                    "psubb {delta_px_xmm}, {last_pixel_xmm}",
                    "movd {delta_px:e}, {delta_px_xmm}",

                    "movd {bias_2_xmm}, {bias:e}",
                    "paddb {bias_2_xmm}, {delta_px_xmm}",
                    "movd {delta_px_b2:e}, {bias_2_xmm}",

                    // input ptrs
                    pixel_ptr = in(reg) &pixel,
                    last_pixel_ptr = in(reg) &prev_pixel,
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
                px_written += 1;
                write_diff(output_buffer, delta_pixel_bias2);
            } else {
                let [dr, dg, db, _] = delta_pixel;
                let dg_bias32 = dg.wrapping_add(32u8);
                let dr_dg_bias8 = dr.wrapping_sub(dg).wrapping_add(8u8);
                let db_dg_bias8 = db.wrapping_sub(dg).wrapping_add(8u8);

                if dg_bias32 < 64 && dr_dg_bias8 < 16 && db_dg_bias8 < 16 {
                    px_written += 1;
                    write_luma(output_buffer, dg_bias32, dr_dg_bias8, db_dg_bias8);
                } else {
                    px_written += 1;
                    write_rgb(output_buffer, pixel);
                }
            }
        }

        prev_pixel = pixel;
    }

    if *pixel_run_counter > 0u8 {
        // end the previous run and keep going down the comparisons
        px_written += *pixel_run_counter as usize;
        write_run(output_buffer, pixel_run_counter);
    }

    if px_written == raw.len() / 4 {
        Ok(px_written)
    } else {
        Err((px_written, raw.len() / 4))
    }
}

fn write_rgba(pixel_buffer: &mut Vec<u8>, pixel: [u8; 4]) {
    pixel_buffer.push(QOI_OP_RGBA);
    pixel_buffer.extend(pixel);
}

fn write_rgb(pixel_buffer: &mut Vec<u8>, pixel: [u8; 4]) {
    pixel_buffer.push(QOI_OP_RGB);
    pixel_buffer.push(pixel[0]);
    pixel_buffer.push(pixel[1]);
    pixel_buffer.push(pixel[2]);
}

fn hash_swap(history_by_hash: &mut [[u8; 4]; 64], pixel: [u8; 4], hash: u8) -> [u8; 4] {
    core::mem::replace(&mut history_by_hash[(hash/*& 0x3f*/) as usize], pixel)
}

fn write_hash_index(pixel_buffer: &mut Vec<u8>, hash: u8) {
    pixel_buffer.push(QOI_OP_INDEX | hash);
}

fn write_diff(pixel_buffer: &mut Vec<u8>, diff_px: [u8; 4]) {
    pixel_buffer.push(QOI_OP_DIFF | diff_px[0] << 4 | diff_px[1] << 2 | diff_px[2]);
}

fn write_luma(pixel_buffer: &mut Vec<u8>, dg: u8, dr_dg: u8, db_dg: u8) {
    pixel_buffer.push(QOI_OP_LUMA | dg);
    pixel_buffer.push((dr_dg << 4) | db_dg);
}

fn write_run(pixel_buffer: &mut Vec<u8>, count: &mut u8) {
    pixel_buffer.push(QOI_OP_RUN | (*count - 1u8)); // 0 means 1 for compactness
    *count = 0u8;
}
