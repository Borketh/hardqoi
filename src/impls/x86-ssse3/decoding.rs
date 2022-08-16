use crate::common::{
    QOIHeader, END_8, QOI_OP_DIFF, QOI_OP_INDEX, QOI_OP_LUMA, QOI_OP_RGB, QOI_OP_RGBA, QOI_OP_RUN,
};
use core::arch::asm;

use super::hashing::{HashIndexedArray, Hashing};

pub fn decode(input: &Vec<u8>, output: &mut Vec<[u8; 4]>) -> Result<(), (usize, usize)> {
    let len: usize = input.len() - 8;
    let mut hash_indexed_array = HashIndexedArray::new();
    let mut last_hash_update = 0;

    let header = QOIHeader::from(<[u8; 14]>::from(input[0..14].try_into().unwrap()));
    let mut pos: usize = 14;
    let mut output_ptr; // = output.as_mut_ptr_range().end;
    let mut previous_pixel_ptr = &[0, 0, 0, 255u8] as *const [u8; 4];

    while pos < len {
        let next_op: u8 = input[pos];
        output_ptr = output.as_mut_ptr_range().end;

        match next_op {
            QOI_OP_RGBA => {
                // look ahead to see if there are multiple
                if input[pos + 5] == QOI_OP_RGBA {
                    let len = output.len();

                    let n_added = 2 + {
                        // whether there are two or three, it still helps
                        let theres_three_actually = input[pos + 10] == QOI_OP_RGBA;
                        theres_three_actually as usize
                    };

                    output.reserve_exact(4);
                    unsafe {
                        load_three_rgba(input.as_ptr().add(pos + 1), output_ptr);
                        output.set_len(len + n_added);
                        previous_pixel_ptr = output_ptr.add(n_added - 1);
                    }

                    pos += 5 * n_added;
                    continue;
                } else {
                    pos += 1;
                    unsafe {
                        load_one_rgba(&input[pos], output_ptr);
                        output.set_len(output.len() + 1);
                    }
                    pos += 4;
                }
            }
            QOI_OP_RGB => {
                pos += 1;
                unsafe {
                    load_one_rgb(&input[pos], previous_pixel_ptr, output_ptr);
                    output.set_len(output.len() + 1);
                }
                pos += 3;
            }
            _ => match next_op & 0b11000000 {
                QOI_OP_DIFF => {
                    let diff = op_diff_expand222(next_op);
                    unsafe {
                        load_one_diff(diff, previous_pixel_ptr, output_ptr);
                        output.set_len(output.len() + 1);
                    }
                    pos += 1;
                }
                QOI_OP_LUMA => {
                    pos += 1;
                    let diff = op_luma_expand644(next_op, input[pos]);
                    unsafe {
                        load_one_luma(diff, previous_pixel_ptr, output_ptr);
                        output.set_len(output.len() + 1);
                    }
                    pos += 1;
                }
                QOI_OP_RUN => {
                    hash_indexed_array.update(&output[last_hash_update..]);
                    let mut run_count = (next_op as usize & 0x3f) + 1;
                    loop {
                        pos += 1;
                        if (QOI_OP_RUN..QOI_OP_RGB).contains(&input[pos]) {
                            let additional = (input[pos] & 0x3f) as usize + 1;
                            run_count += additional;
                        } else {
                            break;
                        }
                    }

                    let cur_len = output.len();
                    output.reserve_exact(run_count + 16);

                    unsafe {
                        load_run(run_count, previous_pixel_ptr, output_ptr);
                        output.set_len(cur_len + run_count);
                    }
                    last_hash_update = output.len(); // no need to repeat hashing updates on the same pixel
                    output_ptr = output.as_mut_ptr_range().end;
                }
                QOI_OP_INDEX => {
                    hash_indexed_array.update(&output[last_hash_update..]);
                    last_hash_update = output.len();

                    let index = next_op & 0b00111111;
                    output.push(hash_indexed_array.fetch(index));
                    pos += 1;
                }
                _ => panic!("YOUR CPU'S AND GATE IS BROKEN"),
            }, // end match 2-bit
        } // end match 8-bit
        previous_pixel_ptr = output_ptr;
    } // end loop

    debug_assert_eq!(
        input[(pos)..(pos + 8)],
        END_8,
        "QOI file does not end normally! Found {:?} instead",
        &input[(pos)..(pos + 8)]
    );

    if header.image_size() == output.len() {
        Ok(())
    } else {
        Err((output.len(), header.image_size()))
    }
}

/// An easily inlinable function that expands the OP_DIFF byte into an array
/// 0b_01_dr_dg_db_u8 -> [000000dr_u8, 000000dg_u8, 000000db_u8, 0u8]
fn op_diff_expand222(x: u8) -> u32 {
    // thanks for this function, https://github.com/adrianparvino ! :)
    let y = (x as u32) * ((1 << 24) | (1 << 14) | (1 << 4));
    (y & 0x03030303) >> 8
}

fn op_luma_expand644(op_and_dg: u8, byte_2: u8) -> u32 {
    let dg_m8 = op_and_dg.wrapping_sub(0b10000000_u8 + 40u8);

    return u32::from_ne_bytes([
        (byte_2 >> 4).wrapping_add(dg_m8),
        dg_m8.wrapping_add(8u8),
        (byte_2 & 0xf).wrapping_add(dg_m8),
        0,
    ]);
}

const RGBA_CHA_CHA: u128 = 0x80808080_0d0c0b0a_08070605_03020100_u128;

#[inline(always)]
unsafe fn load_three_rgba(from: *const u8, to: *mut [u8; 4]) {
    asm!(
        // from points to the first R, so the contents of staging will be either
        // [RGBA ORGB AORG BAXX] or [RGBA ORGB AXXX XXXX]
        "movdqu     {staging},      [{in_ptr}]",
        "movdqu     {shuffler},     [{shuffle_ptr}]",
        "pshufb     {staging},      {shuffler}",
        "movdqu     [{output_ptr}], {staging}",

        in_ptr      = in(reg)       from,
        output_ptr  = in(reg)       to,
        shuffle_ptr = in(reg)       &RGBA_CHA_CHA,

        staging     = out(xmm_reg)  _,
        shuffler    = out(xmm_reg)  _,

        options(nostack, preserves_flags)
    );
}

#[inline(always)]
unsafe fn load_one_rgba(pixel_ptr: &u8, output_ptr: *mut [u8; 4]) {
    asm!(
        "mov        {tmp:e},        [{rgba_ptr}]",
        "mov        [{output}],     {tmp:e}",

        rgba_ptr    = in(reg)       pixel_ptr,
        output      = in(reg)       output_ptr,
        tmp         = out(reg)      _,

        options(nostack, preserves_flags)
    );
}

const RGB_LAST_ALPHA_SWITCHEROO: u128 = 0x80808080_80808080_80808080_05020100_u128;

#[inline(always)]
unsafe fn load_one_rgb(rgbx_ptr: &u8, prev_ptr: *const [u8; 4], output_ptr: *const [u8; 4]) {
    asm!(
        // get the red, green, blue, and a garbage byte
        "movd       {staging},      [{rgbx}]",
        // swipe blue (extraneous) and alpha from the previous pixel
        "pinsrw     {staging},      [{prev} + 2], 2",
        // replace old alpha with new, zeroing everything else
        "movdqu     {shuffler},     [{shuffle_ptr}]",
        "pshufb     {staging},      {shuffler}",
        // put the resulting pixel in to the output buffer
        "movd       [{output}],     {staging}",

        rgbx        = in(reg)       rgbx_ptr,
        prev        = in(reg)       prev_ptr,
        output      = in(reg)       output_ptr,
        shuffle_ptr = in(reg)       &RGB_LAST_ALPHA_SWITCHEROO,

        shuffler    = out(xmm_reg)  _,
        staging     = out(xmm_reg)  _,

        options(nostack, preserves_flags)
    );
}

#[inline(always)]
unsafe fn load_run(length: usize, prev_ptr: *const [u8; 4], mut output_ptr: *mut [u8; 4]) {
    let offset = output_ptr.align_offset(16);

    asm!(
        "movd       xmm0,           [{prev}]",
        "pshufd     xmm0,           xmm0,           0",
        "movdqu     [{output}],     xmm0",

        prev        = in(reg)       prev_ptr,
        output      = in(reg)       output_ptr,

        out("xmm0") _,

        options(nostack, preserves_flags, readonly)
    );

    let remaining_to_splat = length as isize - offset as isize;

    if remaining_to_splat > 0 {
        output_ptr = output_ptr.add(offset);

        let splats_left = (remaining_to_splat & -16isize) >> 4;
        for _ in 0..=splats_left {
            asm!(
                "movdqa [{output}],         xmm0",
                "movdqa [{output} + 16],    xmm0",
                "movdqa [{output} + 32],    xmm0",
                "movdqa [{output} + 48],    xmm0",
                "lea    {output},           [{output} + 4*16]",

                output  = inout(reg)        output_ptr,

                out("xmm0") _,

                options(nostack, preserves_flags)
            );
        }
    }
}

#[inline(always)]
unsafe fn load_one_diff(diff: u32, prev_ptr: *const [u8; 4], output_ptr: *mut [u8; 4]) {
    asm!(
        "movd       {pixel_xmm},    [{prev}]",
        "movd       {diff_xmm},     {diff:e}",
        "paddb      {pixel_xmm},    {diff_xmm}",

        "movd       {bias_xmm},     {bias:e}",
        "psubb      {pixel_xmm},    {bias_xmm}",

        "movd       [{output}],     {pixel_xmm}",

        prev        = in(reg)       prev_ptr,
        diff        = in(reg)       diff,
        bias        = in(reg)       0x00020202_u32,
        output      = in(reg)       output_ptr,

        pixel_xmm   = out(xmm_reg)  _,
        diff_xmm    = out(xmm_reg)  _,
        bias_xmm    = out(xmm_reg)  _,

        options(nostack, preserves_flags)
    )
}

#[inline(always)]
unsafe fn load_one_luma(diff: u32, prev_ptr: *const [u8; 4], output_ptr: *mut [u8; 4]) {
    asm!(
        "movd       {pixel_xmm},    [{prev}]",
        "movd       {diff_xmm},     {diff:e}",
        "paddb      {pixel_xmm},    {diff_xmm}",
        "movd       [{output}],     {pixel_xmm}",

        diff        = in(reg)       diff,
        prev        = in(reg)       prev_ptr,
        output      = in(reg)       output_ptr,

        pixel_xmm   = out(xmm_reg)  _,
        diff_xmm    = out(xmm_reg)  _,

        options(nostack, preserves_flags)
    )
}
