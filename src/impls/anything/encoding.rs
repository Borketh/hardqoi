use bytemuck::cast_slice;
use wrap_math_pixel::{BIAS_2, BLACK, PIXEL, ZERO_PIXEL};

use crate::common::{
    QOIHeader, END_8, MAGIC_QOIF, QOI_OP_DIFF, QOI_OP_INDEX, QOI_OP_LUMA, QOI_OP_RGB, QOI_OP_RGBA,
    QOI_OP_RUN,
};
use crate::hashing::hashes_rgba;

#[path = "wrap_math_pixel.rs"]
mod wrap_math_pixel;

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
    let hashes: Vec<u8> = hashes_rgba(pixels);

    let mut prev_pixel: PIXEL = BLACK;
    let mut hash_indexed_array: [PIXEL; 64] = [ZERO_PIXEL; 64];
    let pixel_run_counter: &mut u8 = &mut 0u8;
    let mut px_written = 0usize;

    for (i, &channels) in pixels.iter().enumerate() {
        let pixel = PIXEL::from(channels);
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
        } else if pixel.a() != prev_pixel.a() {
            px_written += 1;
            write_rgba(output_buffer, pixel);
        } else {
            let delta_pixel: PIXEL = pixel - prev_pixel;

            // pre-biased because that's easier to compare anyway
            let delta_pixel_bias2 = delta_pixel + BIAS_2;
            // if matches!(delta_pixel.rgb_arr(), [0u8..4u8; 3])
            if delta_pixel_bias2.r() < 4u8
                && delta_pixel_bias2.g() < 4u8
                && delta_pixel_bias2.b() < 4u8
            {
                px_written += 1;
                write_diff(output_buffer, delta_pixel_bias2);
            } else {
                let [dr, dg, db] = delta_pixel.rgb_arr();
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

fn write_rgba(pixel_buffer: &mut Vec<u8>, pixel: PIXEL) {
    pixel_buffer.push(QOI_OP_RGBA);
    pixel_buffer.extend(pixel.rgba_arr());
}

fn write_rgb(pixel_buffer: &mut Vec<u8>, pixel: PIXEL) {
    pixel_buffer.push(QOI_OP_RGB);
    pixel_buffer.extend(pixel.rgb_arr());
}

fn hash_swap(history_by_hash: &mut [PIXEL; 64], pixel: PIXEL, hash: u8) -> PIXEL {
    core::mem::replace(&mut history_by_hash[hash as usize], pixel)
}

fn write_hash_index(pixel_buffer: &mut Vec<u8>, hash: u8) {
    pixel_buffer.push(QOI_OP_INDEX | hash);
}

fn write_diff(pixel_buffer: &mut Vec<u8>, diff_px: PIXEL) {
    pixel_buffer.push(QOI_OP_DIFF | diff_px.r() << 4 | diff_px.g() << 2 | diff_px.b());
}

fn write_luma(pixel_buffer: &mut Vec<u8>, dg: u8, dr_dg: u8, db_dg: u8) {
    pixel_buffer.push(QOI_OP_LUMA | dg);
    pixel_buffer.push((dr_dg << 4) | db_dg);
}

fn write_run(pixel_buffer: &mut Vec<u8>, count: &mut u8) {
    pixel_buffer.push(QOI_OP_RUN | (*count - 1u8)); // 0 means 1 for compactness
    *count = 0u8;
}
