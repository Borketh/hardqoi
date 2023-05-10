use alloc::vec::Vec;
use core::arch::x86_64::*;
use core::ops::Range;

use hardqoi::common::{
    QOI_OP_DIFF, QOI_OP_INDEX, QOI_OP_LUMA, QOI_OP_RGB, QOI_OP_RGBA, QOI_OP_RUN, RGBA,
};
use RunResult::{AtLeast, Exactly};

use crate::{BIAS_LUMA, BIAS_RGB};
use crate::{prefetch, rotate};
use crate::common::*;

#[inline(never)]
#[allow(clippy::needless_return)]
pub unsafe fn encode_chunks<const IMAGE_HAS_ALPHA: bool>(
    pixels: &[__m512i],
    mut previous_pixel: *const RGBA,
    hia: &mut [RGBA; 64],
    output_bytes: &mut Vec<u8>,
    mut maybe_run_length: Option<usize>,
) -> (Option<usize>, *const RGBA) {
    let Range {
        start: mut chunk_pointer,
        end: chunk_pointer_max,
    } = pixels.as_ptr_range();

    let mut output_ptr = output_bytes.get_write_head();

    'chunked_encode: // assume all chunks are aligned
    while chunk_pointer < chunk_pointer_max {
        // dereferencing chunks in this way should mean the pixels are only read once per encode.
        let mut rotation = 0u8;
        let mut chunk: __m512i;
        chunk = *chunk_pointer;

        if let Some(run_length) = maybe_run_length.as_mut() {
            'traverse_run: // loop to continue handling a run
            loop {
                let next_chunk_ptr = chunk_pointer.add(1);
                prefetch!(next_chunk_ptr);
                chunk = *chunk_pointer;
                let broadcasted_compare = _mm512_set1_epi32(*previous_pixel as i32);
                let mask = _mm512_cmpneq_epu32_mask(chunk, broadcasted_compare);
                if mask == 0u16 {
                    *run_length += 16;
                    if next_chunk_ptr >= chunk_pointer_max {
                        break 'chunked_encode;
                    }
                    chunk_pointer = next_chunk_ptr;
                } else {
                    let bits = mask.trailing_zeros();  // < 16
                    *run_length += bits as usize;

                    output_bytes.set_len_from_ptr(output_ptr);
                    output_ptr = maybe_write_run(output_bytes, maybe_run_length);

                    // Since we can't rotate by a variable value, we use compress instead
                    rotate!(chunk @ u16::MAX << bits);

                    rotation += bits as u8;
                    maybe_run_length = None;
                    break 'traverse_run;
                }
            }
        }

        output_bytes.set_len_from_ptr(output_ptr);
        output_bytes.reserve(5*16); // maximum non-run capacity necessary
        output_ptr = output_bytes.get_write_head(); // reset in case reserve reallocates

        let hash_multipliers = no_rip_bcst_u32_m512(u32::from_le_bytes([3, 5, 7, 11]));
        let half_done = _mm512_maddubs_epi16(chunk, hash_multipliers);
        let blue_alpha_shifted = _mm512_srli_epi32::<16>(half_done);
        let unmasked_hashes_32b = _mm512_add_epi16(half_done, blue_alpha_shifted);
        let hash_mask = no_rip_bcst_u32_m512(0x3f);
        let mut hashes_32b = _mm512_and_si512(unmasked_hashes_32b, hash_mask);

        'chunk_rotation: // read pixels directly from the chunk in a register
        while rotation < 16 {

            let pixel = _mm512_cvtsi512_si32(chunk) as u32;

            if pixel == *previous_pixel {
                // similar to 'traverse_run but we don't necessarily start on the first
                // pixel of a chunk, so we need to take that into account.
                if (_mm512_cvtsi512_si64(chunk) >> 32) as u32 != pixel {
                    output_ptr = output_ptr.push_var(QOI_OP_RUN);
                    rotate!(chunk, hashes_32b @ once);

                    rotation += 1;
                    continue 'chunk_rotation;
                }
                let broadcasted_compare = _mm512_set1_epi32(*previous_pixel as i32);
                let mask = _mm512_cmpeq_epu32_mask(chunk, broadcasted_compare);
                match run_length(mask, rotation) {
                    AtLeast(amount) => {
                        prefetch!(chunk_pointer + 1);
                        maybe_run_length = Some(amount);
                        break 'chunk_rotation;
                    }
                    Exactly(amount) => {
                        // There will at most be 15 to encode as a run, which fits in one byte.
                        // This means any run encoding can be within the already reserved space.
                        write_run(output_ptr, 0, amount);
                        output_ptr = output_ptr.add(1);

                        // Since we can't rotate by a variable value, we use compress instead
                        rotate!(chunk, hashes_32b @ u16::MAX << amount);

                        rotation += amount as u8;
                        continue 'chunk_rotation;
                    }
                }
            }

            let hash = _mm512_cvtsi512_si32(hashes_32b) as usize;
            // no checks are necessary because the hashes are masked to be < 64
            let hash_index = hia.get_unchecked_mut(hash);

            if pixel == *hash_index {
                output_ptr = output_ptr.push_var(hash as u8 | QOI_OP_INDEX);
                // skip the chaos below and move on
                previous_pixel = hia.get_unchecked(hash);
                rotate!(chunk, hashes_32b @ once);

                rotation += 1;
                continue 'chunk_rotation;
            }
            'encoding_attempt: {
                if
                is_alpha_different::<IMAGE_HAS_ALPHA>(pixel, *previous_pixel) {
                    // all the other methods count on the alpha being the same, so we can't do much else
                    output_ptr = output_ptr.push_var(QOI_OP_RGBA).push_var(pixel);
                    break 'encoding_attempt; // do all the common stuff before moving on
                }

                // try to encode with an OP_DIFF
                let rgb_mask: __mmask16 = 0b111;
                let deltas = {
                    // closure because the names are the same
                    let pixel = xmm_of_low_dword(chunk);
                    let previous_pixel = _mm_cvtsi32_si128(*previous_pixel as i32);
                    actually_kmaskz_subb_si128(rgb_mask, pixel, previous_pixel)
                };

                let bias_rgb = _mm_cvtsi32_si128(BIAS_RGB as i32);
                let limit_rgb = _mm_add_epi8(bias_rgb, bias_rgb);
                let biased_deltas = _mm_add_epi8(deltas, bias_rgb);
                let comparison = _mm_mask_cmplt_epu8_mask(rgb_mask, biased_deltas, limit_rgb);

                if comparison == rgb_mask {
                    // each channel is less than 4
                    let biased_delta = _mm_cvtsi128_si32(biased_deltas) as u32;
                    let correct_order = biased_delta.swap_bytes(); // because it's encoded weird
                    let packed_result = _pext_u32(correct_order, 0x03030300) as u8;

                    output_ptr = output_ptr.push_var(packed_result | QOI_OP_DIFF);
                    break 'encoding_attempt;
                }

                // see if it can be encoded with OP_LUMA instead
                let deltas_luma = {
                    // rotate the deltas such that the channel layout is this:
                    // [the rest of the register] dr __ db dg
                    // this is ideal because we don't have to copy out dg to broadcast it
                    // and it is the same order the encoded product should be if we can use it
                    let rotated_deltas = _mm_ror_epi32::<8>(deltas);
                    let dg = _mm_broadcastb_epi8(rotated_deltas);
                    _mm_mask_sub_epi8(rotated_deltas, 0b1010, rotated_deltas, dg)
                };

                let luma_mask: __mmask16 = 0b1011;
                let bias_luma = _mm_cvtsi32_si128(BIAS_LUMA as i32);
                let limit_luma = _mm_add_epi8(bias_luma, bias_luma);
                let biased_deltas_luma = _mm_add_epi8(deltas_luma, bias_luma);
                let comparison = _mm_mask_cmplt_epu8_mask(luma_mask, biased_deltas_luma, limit_luma);

                if comparison == luma_mask {
                    // each channel is under the limit and we can encode an OP_LUMA
                    let biased_deltas_luma = _mm_cvtsi128_si32(biased_deltas_luma) as u32;
                    let op_luma = _pext_u32(biased_deltas_luma, 0x0f_00_0f_ff) as u16 | QOI_OP_LUMA as u16;
                    output_ptr = output_ptr.push_var(op_luma);
                    break 'encoding_attempt;
                }

                let op_rgb = pixel << 8 | QOI_OP_RGB as u32;
                output_ptr = output_ptr.push_var(op_rgb);
            }


            *hash_index = pixel;
            previous_pixel = hash_index;
            rotate!(chunk, hashes_32b @ once);

            rotation += 1;
        }

        chunk_pointer = chunk_pointer.add(1);
    }
    output_bytes.set_len_from_ptr(output_ptr);
    return (maybe_run_length, previous_pixel);
}

pub enum RunResult {
    AtLeast(usize),
    Exactly(usize),
}

#[inline]
const fn run_length(mask: u16, rotation: u8) -> RunResult {
    let clobber_already_seen = mask << rotation;
    if clobber_already_seen == u16::MAX << rotation {
        AtLeast(16 - rotation as usize)
    } else {
        let clobbered = clobber_already_seen >> rotation;
        let how_many = clobbered.trailing_ones();
        Exactly(how_many as usize)
    }
}
