use crate::common::{
    QOIHeader, END_8, QOI_OP_DIFF, QOI_OP_INDEX, QOI_OP_LUMA, QOI_OP_RGB, QOI_OP_RGBA, QOI_OP_RUN,
};
use core::arch::asm;

use super::hashing::{HashIndexedArray, Hashing};

pub fn decode(input: &Vec<u8>, output: &mut Vec<[u8; 4]>) -> Result<(), (usize, usize)> {
    let len: usize = input.len() - 8;
    let mut prev_pixel: u32 = 0xff000000u32;
    let mut hash_indexed_array = HashIndexedArray::new();
    let mut last_hash_update = 0;

    let header = QOIHeader::from(<[u8; 14]>::from(input[0..14].try_into().unwrap()));
    let mut pos: usize = 14;

    while pos < len {
        let next_op: u8 = input[pos];
        match next_op {
            QOI_OP_RGBA => {
                pos += 1;
                unsafe {
                    asm!(
                        "mov {prev:e}, [{raw_rgba_ptr}]",
                        raw_rgba_ptr = in(reg) &input[pos],
                        prev = out(reg) prev_pixel
                    )
                }
                pos += 4;
            }
            QOI_OP_RGB => {
                pos += 1;
                unsafe {
                    asm!(
                        "mov    {staging:e},    [{rgbx_ptr}]",
                        "and    {staging:e},    16777215",
                        "and    {prev:e},       4278190080",
                        "or     {prev:e},       {staging:e}",
                        staging = out(reg) _,
                        prev = inout(reg) prev_pixel,
                        rgbx_ptr = in(reg) &input[pos]
                    );
                }
                pos += 3;
            }
            _ => match next_op & 0b11000000 {
                QOI_OP_DIFF => {
                    let diff = op_diff_expand222(next_op);
                    unsafe {
                        asm!(
                            "movd   {pixel_xmm},    {px:e}",
                            "movd   {diff_xmm},     {diff:e}",
                            "paddb  {pixel_xmm},    {diff_xmm}",

                            "movd   {bias_xmm},     {bias:e}",
                            "psubb  {pixel_xmm},    {bias_xmm}",

                            "movd   {px:e},         {pixel_xmm}",

                            pixel_xmm = out(xmm_reg) _,
                            diff_xmm = lateout(xmm_reg) _,
                            bias_xmm = out(xmm_reg) _,

                            bias = in(reg) 0x00020202,
                            px = inout(reg) prev_pixel,
                            diff = in(reg) diff
                        )
                    }
                    pos += 1;
                }
                QOI_OP_LUMA => {
                    pos += 1;
                    let diff = op_luma_expand644(next_op, input[pos]);
                    unsafe {
                        asm!(
                            "movd   {pixel_xmm},    {px:e}",
                            "movd   {diff_xmm},     {diff:e}",
                            "paddb  {pixel_xmm},    {diff_xmm}",
                            "movd   {px:e},         {pixel_xmm}",

                            pixel_xmm = out(xmm_reg) _,
                            diff_xmm = lateout(xmm_reg) _,

                            px = inout(reg) prev_pixel,
                            diff = in(reg) diff
                        )
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
                        let mut output_ptr = output.as_mut_ptr().add(cur_len);
                        let offset = output_ptr.align_offset(16);

                        asm!(
                            "movd   xmm0,       {splatee:e}",
                            "pshufd xmm0,       xmm0,           0",
                            "movdqu [{output}], xmm0",
                            splatee = in(reg) prev_pixel,
                            output = in(reg) output_ptr,
                            out("xmm0") _,
                            options(readonly, preserves_flags, nostack)
                        );

                        if run_count > offset {
                            output_ptr = output_ptr.add(offset);

                            let splats_left =
                                (((run_count - offset) & (-16isize as usize)) >> 4) + 1;
                            for _i in 0..splats_left {
                                asm!(
                                    "movdqa [{output}],      xmm0",
                                    "movdqa [{output} + 16], xmm0",
                                    "movdqa [{output} + 32], xmm0",
                                    "movdqa [{output} + 48], xmm0",
                                    output = in(reg) output_ptr,
                                    out("xmm0") _,
                                    options(nostack, preserves_flags)
                                );
                                output_ptr = output_ptr.add(16);
                            }
                        }

                        output.set_len(cur_len + run_count);
                    }
                    last_hash_update = output.len(); // no need to repeat hashing updates on the same pixel
                    continue;
                }
                QOI_OP_INDEX => {
                    hash_indexed_array.update(&output[last_hash_update..]);
                    last_hash_update = output.len();

                    let index = next_op & 0b00111111;
                    prev_pixel = u32::from_ne_bytes(hash_indexed_array.fetch(index));
                    pos += 1;
                }
                _ => panic!("YOUR CPU'S AND GATE IS BROKEN"),
            }, // end match 2-bit
        } // end match 8-bit
        output.push(prev_pixel.to_ne_bytes());
    } // end loop

    assert_eq!(
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
