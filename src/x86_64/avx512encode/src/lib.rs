#![no_std]
#![feature(stdsimd, avx512_target_feature)]

extern crate alloc;

use alloc::vec::Vec;
use core::arch::x86_64::*;

use common::{NoPushByteWrite, Util};
use hardqoi::common::{
    END_8, QOI_OP_DIFF, QOI_OP_INDEX, QOI_OP_LUMA, QOI_OP_RGB, QOI_OP_RGBA, QOIHeader, RGBA,
};

mod mk1;
#[macro_use]
pub(crate) mod common;

const BIAS_RGB: u32 = 0x020202;
const BIAS_LUMA: u32 = u32::from_be_bytes([8, 0, 8, 32]);

pub fn encode(
    input_pixels: &Vec<RGBA>,
    output_bytes: &mut Vec<u8>,
    metadata: QOIHeader,
) -> Result<(), (usize, usize)> {
    debug_assert_eq!(input_pixels.len(), metadata.image_size());

    let mut previous_pixel: *const RGBA = &0xff000000u32;
    let mut hia = [0; 64];
    let mut maybe_run_length: Option<usize> = None;

    let (unaligned_start, aligned_pixels, unaligned_end) =
        unsafe { input_pixels.align_to::<__m512i>() };

    output_bytes.extend(metadata.to_bytes());
    if metadata.has_alpha {
        if !unaligned_start.is_empty() {
            (maybe_run_length, previous_pixel) = encode_singles::<true>(
                unaligned_start,
                previous_pixel,
                &mut hia,
                output_bytes,
                maybe_run_length,
            );
        }

        (maybe_run_length, previous_pixel) = unsafe {
            mk1::encode_chunks::<true>(
                aligned_pixels,
                previous_pixel,
                &mut hia,
                output_bytes,
                maybe_run_length,
            )
        };

        if !unaligned_end.is_empty() {
            (maybe_run_length, _) = encode_singles::<true>(
                unaligned_end,
                previous_pixel,
                &mut hia,
                output_bytes,
                maybe_run_length,
            );
        }
    } else {
        if !unaligned_start.is_empty() {
            (maybe_run_length, previous_pixel) = encode_singles::<false>(
                unaligned_start,
                previous_pixel,
                &mut hia,
                output_bytes,
                maybe_run_length,
            );
        }

        (maybe_run_length, previous_pixel) = unsafe {
            mk1::encode_chunks::<false>(
                aligned_pixels,
                previous_pixel,
                &mut hia,
                output_bytes,
                maybe_run_length,
            )
        };

        if !unaligned_end.is_empty() {
            (maybe_run_length, _) = encode_singles::<false>(
                unaligned_end,
                previous_pixel,
                &mut hia,
                output_bytes,
                maybe_run_length,
            );
        }
    };

    unsafe {
        common::maybe_write_run(output_bytes, maybe_run_length);
    }

    output_bytes.extend(END_8);
    Ok(())
}

#[allow(clippy::needless_return)]
// The most this will encode is 15 at any given time, so optimizing here isn't really useful.
fn encode_singles<const IMAGE_HAS_ALPHA: bool>(
    pixels: &[RGBA],
    mut previous_pixel: *const RGBA,
    hia: &mut [RGBA; 64],
    output_bytes: &mut Vec<u8>,
    mut maybe_run_length: Option<usize>,
) -> (Option<usize>, *const RGBA) {
    // temporary write space with enough space for 15 OP_RGBAs in case an image is that stubborn
    output_bytes.reserve_exact(15 * 5);
    let mut output_ptr = unsafe { output_bytes.get_write_head() };

    'encoding_next_pixel: // this label is just for clarity
    for &pixel in pixels {
        // dbg!(pixel.to_le_bytes());
        if pixel == unsafe {*previous_pixel} {
            // Start or continue a run.
            *maybe_run_length.get_or_insert(0) += 1;
            // println!("This pixel is the same as the previous pixel ({:?})", (*previous_pixel).to_ne_bytes());
            // Since no new pixels are here, the hash table and previous pixel won't need updating.
            continue 'encoding_next_pixel;
        } else if maybe_run_length.is_some() {
            // We need to finish of the run for the previous pixels.
            // Since this function will only look at 15 pixels at a time, if a run terminates
            // within this function we only need one byte to encode it.
            // println!("This pixel is different, so we're finishing up a RUN");
            unsafe {
                output_bytes.set_len_from_ptr(output_ptr);
                output_ptr = common::maybe_write_run(output_bytes, maybe_run_length);
            }
            maybe_run_length = None;

            // We still need to deal with the current pixel now
        }

        let hash = common::hash_rgba(pixel);
        // no checks are necessary because the hashes are masked to be < 64
        let hash_index = unsafe { hia.get_unchecked_mut(hash as usize) };

        if pixel == *hash_index {
            unsafe {
                output_ptr = output_ptr.push_var(hash | QOI_OP_INDEX);
                previous_pixel = hash_index;
            }
            continue 'encoding_next_pixel;
        }


        if common::is_alpha_different::<IMAGE_HAS_ALPHA>(pixel, unsafe {*previous_pixel}) {
            // all the other methods count on the alpha being the same, so we can't do much else
            unsafe {
                output_ptr = output_ptr.push_var(QOI_OP_RGBA).push_var(pixel);
            }
            *hash_index = pixel;
            previous_pixel = hash_index;
            continue 'encoding_next_pixel;
        }

        // try to encode with an OP_DIFF
        let rgb_mask = 0b111;
        let deltas = unsafe {
            let pixel = _mm_cvtsi32_si128(pixel as i32);
            let previous_pixel = _mm_cvtsi32_si128(*previous_pixel as i32);
            common::actually_kmaskz_subb_si128(rgb_mask, pixel, previous_pixel)
        };

        let (biased_deltas, comparison) = unsafe {
            let bias_rgb = _mm_cvtsi32_si128(BIAS_RGB as i32);
            let limit_rgb = _mm_add_epi8(bias_rgb, bias_rgb);
            let biased_deltas = _mm_add_epi8(deltas, bias_rgb);
            let comparison = _mm_mask_cmplt_epu8_mask(rgb_mask, biased_deltas, limit_rgb);
            (biased_deltas, comparison)
        };
        if comparison == rgb_mask {
            // each channel is less than 4
            let packed_result = unsafe {
                let biased_delta = _mm_cvtsi128_si32(biased_deltas) as u32;
                let correct_order = biased_delta.swap_bytes(); // because it's encoded weird
                _pext_u32(correct_order, 0x03030300) as u8
            };
            unsafe {
                output_ptr = output_ptr.push_var(packed_result | QOI_OP_DIFF);
            }
            *hash_index = pixel;
            previous_pixel = hash_index;

            continue 'encoding_next_pixel;
        }

        // see if it can be encoded with OP_LUMA instead
        let deltas_luma = unsafe {
            // rotate the deltas such that the channel layout is this:
            // [the rest of the register] dr __ db dg
            // this is ideal because we don't have to copy out dg to broadcast it
            // and it is the same order the encoded product should be if we can use it
            let rotated_deltas = _mm_ror_epi32::<8>(deltas);
            let dg = _mm_broadcastb_epi8(rotated_deltas);
            _mm_mask_sub_epi8(rotated_deltas, 0b1010, rotated_deltas, dg)
        };

        let (biased_deltas_luma, comparison) = unsafe {
            let bias_luma = _mm_cvtsi32_si128(BIAS_LUMA as i32);
            let limit_luma = _mm_add_epi8(bias_luma, bias_luma);
            let biased_deltas_luma = _mm_add_epi8(deltas_luma, bias_luma);
            let comparison = _mm_mask_cmplt_epu8_mask(0b1011, biased_deltas_luma, limit_luma);
            (biased_deltas_luma, comparison)
        };

        if comparison == 0b1011 {
            // each channel is under the limit
            let (dg_db, dr) = unsafe {
                (
                    _mm_extract_epi16::<0>(biased_deltas_luma) as u16,
                    _mm_extract_epi16::<1>(biased_deltas_luma) as u16,
                )
            };
            let op_luma = dg_db | QOI_OP_LUMA as u16 | (dr << 4);
            unsafe {
                output_ptr = output_ptr.push_var(op_luma)
            }
        } else {
            let op_rgb = pixel << 8 | QOI_OP_RGB as u32;
            unsafe {
                output_ptr = output_ptr.push_var(op_rgb);
            }
        }
        *hash_index = pixel;
        previous_pixel = hash_index;
    }
    unsafe { output_bytes.set_len_from_ptr(output_ptr) }

    return (maybe_run_length, previous_pixel);
}
