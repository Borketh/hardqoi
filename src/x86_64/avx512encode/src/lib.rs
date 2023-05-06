#![no_std]
#![feature(stdsimd, portable_simd, repr_simd, avx512_target_feature)]

extern crate alloc;

use alloc::vec::Vec;
use core::arch::x86_64::*;
use core::ops::Range;
use hardqoi::common::*;

pub fn encode(
    input_pixels: &Vec<RGBA>,
    output_bytes: &mut Vec<u8>,
    metadata: QOIHeader,
) -> Result<(), (usize, usize)> {
    debug_assert_eq!(input_pixels.len(), metadata.image_size());

    let mut previous_pixel: RGBA = 0xff000000u32;
    let mut hia = [0; 64];
    //hia[hash_rgba(previous_pixel) as usize] = previous_pixel;

    let (unaligned_start, aligned_pixels, unaligned_end) =
        unsafe { input_pixels.align_to::<__m512i>() };

    output_bytes.extend(metadata.to_bytes());

    // dbg!(
    //     unaligned_start.len(),
    //     aligned_pixels.len() * 16,
    //     unaligned_end.len()
    // );
    let mut maybe_run_length: Option<usize> = None;

    if !unaligned_start.is_empty() {
        maybe_run_length = encode_singles(
            unaligned_start,
            &mut previous_pixel,
            &mut hia,
            output_bytes,
            maybe_run_length,
        );
    }

    maybe_run_length = unsafe {
        encode_chunks(
            aligned_pixels,
            &mut previous_pixel,
            &mut hia,
            output_bytes,
            maybe_run_length,
        )
    };

    if !unaligned_end.is_empty() {
        maybe_run_length = encode_singles(
            unaligned_end,
            &mut previous_pixel,
            &mut hia,
            output_bytes,
            maybe_run_length,
        );
    }

    maybe_write_run(output_bytes, maybe_run_length);

    output_bytes.extend(END_8);
    Ok(())
}

#[allow(clippy::needless_return)]
// The most this will encode is 15 at any given time, so optimizing here isn't really useful.
fn encode_singles(
    pixels: &[RGBA],
    previous_pixel: &mut RGBA,
    hia: &mut [RGBA; 64],
    output_bytes: &mut Vec<u8>,
    mut maybe_run_length: Option<usize>,
) -> Option<usize> {
    // temporary write space with enough space for 15 OP_RGBAs in case an image is that stubborn
    output_bytes.reserve_exact(15 * 5);
    let mut out_ptr = unsafe { output_bytes.as_mut_ptr().add(output_bytes.len()) };

    'encoding_next_pixel: // this label is just for clarity
    for &pixel in pixels {
        // dbg!(pixel.to_le_bytes());
        if pixel == *previous_pixel {
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
                output_bytes.set_len(out_ptr.offset_from(output_bytes.as_ptr()) as usize);
            }
            out_ptr = maybe_write_run(output_bytes, maybe_run_length);

            maybe_run_length = None;

            // We still need to deal with the current pixel now
        }

        let hash = hash_rgba(pixel);
        if pixel == hia[hash as usize] {
            // println!("This pixel is one that has remained in the hash cache.");
            unsafe {
                out_ptr.write(hash | QOI_OP_INDEX);
                out_ptr = out_ptr.add(1);
            }
            *previous_pixel = pixel;
            continue 'encoding_next_pixel;
        }

        if is_alpha_different(pixel, *previous_pixel) {
            // all the other methods count on the alpha being the same, so we can't do much else
            // yeet the pixel threefold
            // println!("This pixel has a different alpha value than the previous one.");
            unsafe {
                out_ptr.write(QOI_OP_RGBA);
                (out_ptr.add(1) as *mut u32).write(pixel);
                out_ptr = out_ptr.add(5);
            }
            hia[hash as usize] = pixel;
            *previous_pixel = pixel;
            continue 'encoding_next_pixel;
        }

        // try to encode with an OP_DIFF
        let rgb_mask = 0b111;
        let deltas = unsafe {
            let pixel = _mm_cvtsi32_si128(pixel as i32);
            let previous_pixel = _mm_cvtsi32_si128(*previous_pixel as i32);
            _mm_maskz_sub_epi8(rgb_mask, pixel, previous_pixel)
        };

        // println!("The delta from the last pixel is {:?}", unsafe {
        //     let tmp = _mm_cvtsi128_si32(deltas).to_le_bytes();
        //     (tmp[0] as i8, tmp[1] as i8, tmp[2] as i8)
        // });

        let (biased_deltas, comparison) = unsafe {
            let bias_rgb = _mm_cvtsi32_si128(0x00020202);
            let biased_deltas = _mm_add_epi8(deltas, bias_rgb);
            let limit_rgb = _mm_add_epi8(bias_rgb, bias_rgb);
            let comparison = _mm_mask_cmplt_epu8_mask(rgb_mask, biased_deltas, limit_rgb);
            // println!("{:?} < {:?} ?= {:016b}",
            //          *(&biased_deltas as *const __m128i as *const [u8; 16]),
            //          *(&limit_rgb as *const __m128i as *const [u8; 16]),
            //          comparison.reverse_bits());
            (biased_deltas, comparison)
        };
        if comparison == rgb_mask {
            // println!("This pixel is encodable with a DIFF");
            // each channel is less than 4
            let packed_result = unsafe {
                let biased_delta = _mm_cvtsi128_si32(biased_deltas) as u32;
                let correct_order = biased_delta.swap_bytes(); // because it's encoded weird
                _pext_u32(correct_order, 0x03030300) as u8
            };
            unsafe {
                out_ptr.write(packed_result | QOI_OP_DIFF);
                out_ptr = out_ptr.add(1);
            }
            hia[hash as usize] = pixel;
            *previous_pixel = pixel;

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
            let bias_luma = _mm_cvtsi32_si128(i32::from_be_bytes([8, 0, 8, 32]));
            let limit_luma = _mm_add_epi8(bias_luma, bias_luma);
            let biased_deltas_luma = _mm_add_epi8(deltas_luma, bias_luma);
            let comparison = _mm_mask_cmplt_epu8_mask(0b1011, biased_deltas_luma, limit_luma);
            // println!("{:?} < {:?} ?= {:016b}",
            //          *(&biased_deltas_luma as *const __m128i as *const [u8; 16]),
            //          *(&limit_luma as *const __m128i as *const [u8; 16]),
            //          comparison.reverse_bits());
            (biased_deltas_luma, comparison)
        };
        if comparison == 0b1011 {
            // println!("This pixel is encodable as a LUMA");
            // each channel is under the limit
            let (dg_db, dr) = unsafe {
                (
                    _mm_extract_epi16::<0>(biased_deltas_luma) as u16,
                    _mm_extract_epi16::<1>(biased_deltas_luma) as u16,
                )
            };
            let op_luma = dg_db | QOI_OP_LUMA as u16 | (dr << 4);
            unsafe {
                (out_ptr as *mut u16).write(op_luma);
                out_ptr = out_ptr.add(2);
            }
        } else {
            // println!("This pixel is encodable with a RGB");
            let op_rgb = pixel << 8 | QOI_OP_RGB as u32;
            // println!("{:02x?}, {:08x}", op_rgb.to_le_bytes(), op_rgb);
            unsafe {
                (out_ptr as *mut u32).write(op_rgb);
                out_ptr = out_ptr.add(4);
            }
        }
        hia[hash as usize] = pixel;
        *previous_pixel = pixel;
    }
    unsafe { output_bytes.set_len(out_ptr.offset_from(output_bytes.as_ptr()) as usize) }
    // println!("{:02x?}", output_bytes);
    // panic!("whoopsie");
    return maybe_run_length;
}

#[inline(never)]
#[allow(clippy::needless_return)]
unsafe fn encode_chunks(
    pixels: &[__m512i],
    previous_pixel: &mut RGBA,
    hia: &mut [RGBA; 64],
    output_bytes: &mut Vec<u8>,
    mut maybe_run_length: Option<usize>,
) -> Option<usize> {
    let Range {
        start: mut chunk_pointer,
        end: chunk_pointer_max,
    } = pixels.as_ptr_range();

    output_bytes.reserve(pixels.len() * 2);
    // dbg!(chunk_pointer_max.offset_from(chunk_pointer));
    'chunked_encode: // assume all chunks are aligned
    while chunk_pointer < chunk_pointer_max {
        // dereferencing chunks in this way should mean the pixels are only read once per encode.
        let mut rotation = 0u8;
        let mut chunk: __m512i;
        chunk = *chunk_pointer;

        if let Some(run_length) = maybe_run_length.as_mut() {
            'traverse_run: // loop to continue handling a run
            loop {
                chunk = *chunk_pointer;
                let broadcasted_compare = _mm512_set1_epi32(*previous_pixel as i32);
                let mask = _mm512_cmpeq_epu32_mask(chunk, broadcasted_compare);
                let bits = mask.trailing_ones();
                *run_length += bits as usize;
                if bits < 16 {
                    write_run(output_bytes, *run_length / 62, *run_length % 62);

                    // Since we can't rotate by a variable value, we use compress and expand instead
                    let shifted_left = _mm512_maskz_compress_epi32(u16::MAX << bits, chunk);
                    chunk = _mm512_mask_expand_epi32(shifted_left, !(u16::MAX >> bits), chunk);

                    rotation += bits as u8;
                    maybe_run_length = None;
                    break 'traverse_run;
                } else {
                    chunk_pointer = chunk_pointer.add(1);
                    if chunk_pointer >= chunk_pointer_max {
                        break 'chunked_encode;
                    }
                }
            }
        }

        output_bytes.reserve(5*16); // maximum non-run capacity necessary

        let hash_multipliers: __m512i = _mm512_set1_epi32(i32::from_le_bytes([3, 5, 7, 11]));
        let half_done = _mm512_maddubs_epi16(chunk, hash_multipliers);
        let blue_alpha_shifted = _mm512_srli_epi32::<16>(half_done);
        let unmasked_hashes_32b = _mm512_maskz_add_epi16(0x55555555, half_done, blue_alpha_shifted);
        let per_hash_round_mask = _mm512_set1_epi32(0x0000003f);
        let mut hashes_32b = _mm512_and_si512(unmasked_hashes_32b, per_hash_round_mask);

        'chunk_rotation: // read pixels directly from the chunk in a register
        while rotation < 16 {

            let pixel = _mm512_cvtsi512_si32(chunk) as u32;

            if pixel == *previous_pixel {
                // similar to 'traverse_run but we don't necessarily start on the first
                // pixel of a chunk, so we need to take that into account.
                let broadcasted_compare = _mm512_set1_epi32(*previous_pixel as i32);
                let mask = _mm512_cmpeq_epu32_mask(chunk, broadcasted_compare);
                let (bits, possibly_more) = run_length(mask, rotation);
                if possibly_more {
                    maybe_run_length = Some(bits);
                    break 'chunk_rotation;
                } else {
                    // there will at most be 15
                    write_run(output_bytes, 0, bits);
                    let chunk_shifted_left = _mm512_maskz_compress_epi32(u16::MAX << bits, chunk);
                    chunk = _mm512_mask_expand_epi32(chunk_shifted_left, !(u16::MAX >> bits), chunk);

                    // Since we can't rotate by a variable value, we use compress and expand instead
                    let hashes_shifted_left = _mm512_maskz_compress_epi32(u16::MAX << bits, hashes_32b);
                    hashes_32b = _mm512_mask_expand_epi32(hashes_shifted_left, !(u16::MAX >> bits), hashes_32b);

                    rotation += bits as u8;
                    continue 'chunk_rotation;
                }

            }

            let hash = _mm512_cvtsi512_si32(hashes_32b) as usize;

            if pixel == hia[hash] {
                output_bytes.push(hash as u8 | QOI_OP_INDEX);
                // skip the chaos below and move on
                *previous_pixel = pixel;
                chunk = _mm512_alignr_epi32::<1>(chunk, chunk);
                hashes_32b = _mm512_alignr_epi32::<1>(hashes_32b, hashes_32b);
                rotation += 1;
                continue 'chunk_rotation;
            }
            'encoding_attempt: {
                if is_alpha_different(pixel, *previous_pixel) {
                    // all the other methods count on the alpha being the same, so we can't do much else
                    output_bytes.push(QOI_OP_RGBA);
                    output_bytes.extend(pixel.to_le_bytes());
                    break 'encoding_attempt;
                }

                // try to encode with an OP_DIFF
                let rgb_mask = 0b111;
                let deltas = {
                    let pixel = _mm_cvtsi32_si128(pixel as i32);
                    let previous_pixel = _mm_cvtsi32_si128(*previous_pixel as i32);
                    _mm_maskz_sub_epi8(rgb_mask, pixel, previous_pixel)
                };

                let bias_rgb = _mm_cvtsi32_si128(0x00020202);
                let biased_deltas = _mm_add_epi8(deltas, bias_rgb);
                let limit_rgb = _mm_add_epi8(bias_rgb, bias_rgb);
                let comparison = _mm_mask_cmplt_epu8_mask(rgb_mask, biased_deltas, limit_rgb);

                if comparison == rgb_mask {
                    // each channel is less than 4
                    let biased_delta = _mm_cvtsi128_si32(biased_deltas) as u32;
                    let correct_order = biased_delta.swap_bytes(); // because it's encoded weird
                    let packed_result = _pext_u32(correct_order, 0x03030300) as u8;

                    output_bytes.push(packed_result | QOI_OP_DIFF);
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
                let (biased_deltas_luma, comparison) = {
                    let bias_luma = _mm_cvtsi32_si128(i32::from_be_bytes([8, 0, 8, 32]));
                    let limit_luma = _mm_add_epi8(bias_luma, bias_luma);
                    let biased_deltas_luma = _mm_add_epi8(deltas_luma, bias_luma);
                    let comparison = _mm_mask_cmplt_epu8_mask(0b1011, biased_deltas_luma, limit_luma);
                    (biased_deltas_luma, comparison)
                };
                if comparison == 0b1011 {
                    // each channel is under the limit and we can encode an OP_LUMA
                    let dg_db = _mm_extract_epi16::<0>(biased_deltas_luma) as u16;
                    let dr = _mm_extract_epi16::<1>(biased_deltas_luma) as u16;

                    let op_luma = dg_db | QOI_OP_LUMA as u16 | (dr << 4);
                    output_bytes.extend(op_luma.to_le_bytes());
                    break 'encoding_attempt;

                }

                let op_rgb = pixel << 8 | QOI_OP_RGB as u32;
                output_bytes.extend(op_rgb.to_le_bytes());

            }

            hia[hash] = pixel;
            *previous_pixel = pixel;
            chunk = _mm512_alignr_epi32::<1>(chunk, chunk);
            hashes_32b = _mm512_alignr_epi32::<1>(hashes_32b, hashes_32b);

            rotation += 1;
        }

        chunk_pointer = chunk_pointer.add(1);
    }
    return maybe_run_length;
}

#[inline]
const fn run_length(mask: u16, rotation: u8) -> (usize, bool) {
    let clobber_already_seen = mask << rotation;
    let clobbered = clobber_already_seen >> rotation;
    let trues_from_start = clobbered.trailing_ones();
    let may_surpass = clobber_already_seen == u16::MAX << rotation;
    (trues_from_start as usize, may_surpass)
}

#[inline]
const fn is_alpha_different(a: u32, b: u32) -> bool {
    (a & 0xff000000) != (b & 0xff000000)
}

#[inline]
const fn hash_rgba(pixel: RGBA) -> HASH {
    let pixel = pixel as u64;

    // the first two lines do the same as rapid-qoi
    let duplicated = pixel.wrapping_mul(0x0000000100000001_u64);
    let a0g00b0r = duplicated & 0xff00ff0000ff00ff_u64;
    // this magic number puts the hash in the top 6 bits instead of the top 8
    let hash_high6 = a0g00b0r.wrapping_mul(0x0c001c000014002c_u64);
    let hash = hash_high6 >> 58; // now there's no need for the last mask

    hash as HASH
}

#[inline(always)]
fn maybe_write_run(output_bytes: &mut Vec<u8>, maybe_run_length: Option<usize>) -> *mut u8 {
    if let Some(run_length) = maybe_run_length {
        let full_runs = run_length / 62;
        write_run(output_bytes, full_runs, run_length % 62)
    }
    unsafe { output_bytes.as_mut_ptr().add(output_bytes.len()) }
}

#[inline(always)]
fn write_run(output_bytes: &mut Vec<u8>, full_runs: usize, remainder: usize) {
    let rem_op = QOI_OP_RUN | ((remainder as u8).wrapping_sub(1) & !QOI_OP_RUN);

    if full_runs > 0 {
        output_bytes.reserve_exact(full_runs);
        unsafe {
            output_bytes
                .as_mut_ptr()
                .add(output_bytes.len())
                .write_bytes(0xfdu8, full_runs);
            output_bytes.set_len(output_bytes.len() + full_runs);
        }
        if remainder != 0 {
            output_bytes.push(rem_op);
        }
    } else {
        debug_assert_ne!(remainder, 0, "Somehow this got called on a run of 0");
        output_bytes.push(rem_op);
    }
}
