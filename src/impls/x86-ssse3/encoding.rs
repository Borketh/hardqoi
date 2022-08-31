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
    let pixels: &[u32] = cast_slice::<u8, u32>(raw);
    debug_assert_eq!(size, pixels.len());

    let hashes: Vec<u8> = hashes_rgba(raw, pixels.len());

    let mut hash_indexed_array: [u32; 64] = [0u32; 64];
    let mut pos = 0;
    let mut prev_pixel: &u32 = &0xff000000;

    while pos < size {
        let pixel = pixels[pos];
        let pixel_of_same_hash = hash_swap(&mut hash_indexed_array, pixel, hashes[pos]);

        if pixel == *prev_pixel {
            unsafe {
                let current_ptr = pixels.as_ptr().add(pos);
                let (max_runs, last_run) = find_run_length(current_ptr, &mut pos, size);
                write_run(output_buffer, max_runs, last_run);
            }
            continue;
        }

        if pixel == pixel_of_same_hash {
            write_hash_index(output_buffer, hashes[pos]);
        } else if (pixel & 0xff000000) != (*prev_pixel & 0xff000000) {
            write_rgba(output_buffer, pixel);
        } else {
            let mut delta_pixel: u32;

            // pre-biased because that's easier to compare anyway
            let mut delta_pixel_bias2: u32;
            unsafe {
                asm!(
                    "movd {delta_px_xmm}, {pixel:e}",
                    "movd {last_pixel_xmm}, [{last_pixel_ptr}]",
                    "psubb {delta_px_xmm}, {last_pixel_xmm}",
                    "movd {delta_px:e}, {delta_px_xmm}",

                    "movd {bias_2_xmm}, {bias:e}",
                    "paddb {bias_2_xmm}, {delta_px_xmm}",
                    "movd {delta_px_b2:e}, {bias_2_xmm}",

                    // input
                    pixel = in(reg) pixel,
                    last_pixel_ptr = in(reg) prev_pixel,
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
                write_diff(output_buffer, delta_pixel_bias2);
            } else {
                let [dr, dg, db, _] = delta_pixel;
                let dg_bias32 = dg.wrapping_add(32u8);
                let dr_dg_bias8 = dr.wrapping_sub(dg).wrapping_add(8u8);
                let db_dg_bias8 = db.wrapping_sub(dg).wrapping_add(8u8);

                if dg_bias32 < 64 && dr_dg_bias8 < 16 && db_dg_bias8 < 16 {
                    write_luma(output_buffer, dg_bias32, dr_dg_bias8, db_dg_bias8);
                } else {
                    write_rgb(output_buffer, pixel);
                }
            }
        }

        prev_pixel = &pixels[pos];
        pos += 1;
    }

    if pos == size {
        Ok(pos)
    } else {
        Err((pos, size))
    }
}

fn write_rgba(encoded_buf: &mut Vec<u8>, pixel: u32) {
    encoded_buf.push(QOI_OP_RGBA);
    encoded_buf.extend(pixel.to_ne_bytes());
}

fn write_rgb(encoded_buf: &mut Vec<u8>, pixel: u32) {
    encoded_buf.extend(((pixel << 8) | QOI_OP_RGB as u32).to_ne_bytes())
}

fn hash_swap(history_by_hash: &mut [u32; 64], pixel: u32, hash: u8) -> u32 {
    debug_assert!(hash < 64);
    core::mem::replace(&mut history_by_hash[hash as usize], pixel)
}

fn write_hash_index(encoded_buf: &mut Vec<u8>, hash: u8) {
    encoded_buf.push(QOI_OP_INDEX | hash);
}

fn write_diff(encoded_buf: &mut Vec<u8>, diff_px: [u8; 4]) {
    encoded_buf.push(QOI_OP_DIFF | diff_px[0] << 4 | diff_px[1] << 2 | diff_px[2]);
}

fn write_luma(encoded_buf: &mut Vec<u8>, dg: u8, dr_dg: u8, db_dg: u8) {
    encoded_buf.push(QOI_OP_LUMA | dg);
    encoded_buf.push((dr_dg << 4) | db_dg);
}

#[inline(always)]
unsafe fn write_run(encoded_buf: &mut Vec<u8>, max_runs: usize, remainder: usize) {
    let rem_op = QOI_OP_RUN | ((remainder as u8).wrapping_sub(1) & !QOI_OP_RUN);
    let additional = max_runs;

    if max_runs > 0 {
        encoded_buf.reserve_exact(additional);
        let start_ptr = encoded_buf.as_mut_ptr_range().end;
        asm!(
            "cld",
            "rep stosb",
            inout("rcx") additional => _,
            inout("rdi") start_ptr => _,
            in("al") 0xfdu8,
        );
        encoded_buf.set_len(encoded_buf.len() + additional);
        if remainder != 0 {
            encoded_buf.push(rem_op);
        }
    } else {
        encoded_buf.push(rem_op);
    }
}

#[inline(always)]
unsafe fn find_run_length(start_ptr: *const u32, pos: &mut usize, size: usize) -> (usize, usize) {
    let mut end_ptr: *const u32;

    asm!(
        "cld",
        "mov eax, [rdi]",
        "repe scasd",
        inout("rdi") start_ptr => end_ptr,
        inout("rcx") (size - *pos) + 1 => _,
        out("eax") _
    );

    let actual_end_ptr = end_ptr.sub(1);

    let total_run_length = actual_end_ptr.offset_from(start_ptr);
    debug_assert!(total_run_length > 0);
    let total_run_length = total_run_length as usize;
    *pos += total_run_length;

    let n_max_runs = total_run_length / 62;
    let remaining_run = total_run_length % 62;

    (n_max_runs, remaining_run)
}
