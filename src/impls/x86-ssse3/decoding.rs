use crate::common::{
    QOIHeader, END_8, QOI_OP_DIFF, QOI_OP_INDEX, QOI_OP_LUMA, QOI_OP_RGB, QOI_OP_RGBA, QOI_OP_RUN,
};
use core::arch::asm;

use super::hashing::{HashIndexedArray, Hashing};

#[inline(never)]
pub fn decode(input: &Vec<u8>, output: &mut Vec<[u8; 4]>) -> Result<(), (usize, usize)> {
    let len: usize = input.len() - 8;
    let mut hash_indexed_array = HashIndexedArray::new();
    let mut last_hash_update = 0;

    let header = QOIHeader::from(<[u8; 14]>::from(input[0..14].try_into().unwrap()));
    output.reserve_exact(header.image_size());
    let mut pos: usize = 14;
    let mut output_ptr: *mut [u8; 4];
    let mut previous_pixel_ptr: *const [u8; 4] = &[0, 0, 0, 255u8];

    // if the first op is a run, black ends up not in the HIA because of the hash-skipping behaviour
    if (QOI_OP_RUN..QOI_OP_RGB).contains(&input[pos]) {
        // this fixes that
        hash_indexed_array.push(unsafe { *previous_pixel_ptr });
    }

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
                    unsafe {
                        load_one_luma(
                            input.as_ptr().add(pos) as *const u16,
                            previous_pixel_ptr,
                            output_ptr,
                        );
                        output.set_len(output.len() + 1);
                    }
                    pos += 2;
                }
                QOI_OP_RUN => {
                    hash_indexed_array.update(&output[last_hash_update..]);

                    unsafe {
                        let run_count = find_run_length(input.as_ptr().add(pos), &mut pos, len);
                        let new_len = output.len() + run_count;

                        load_run(run_count, previous_pixel_ptr, output_ptr);
                        output.set_len(new_len);

                        output_ptr = output.as_mut_ptr().add(new_len);
                        last_hash_update = new_len; // no need to repeat hashing updates on the same pixel
                    }
                }
                QOI_OP_INDEX => {
                    hash_indexed_array.update(&output[last_hash_update..]);
                    last_hash_update = output.len();

                    let index = next_op & 0x3f;
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
fn length_from_op_run(op_run: u8) -> usize {
    return (op_run & !QOI_OP_RUN) as usize + 1;
}

unsafe fn find_run_length(start_ptr: *const u8, pos: &mut usize, size: usize) -> usize {
    let mut end_ptr: *const u8;

    asm!(
        "cld",
        "repe scasb",

        in("al") 0xfdu8,
        inout("rdi") start_ptr => end_ptr,
        inout("rcx") (size - *pos) + 1 => _
    );

    let actual_end_ptr = end_ptr.sub(1);
    let number_of_62s = actual_end_ptr as usize - start_ptr as usize;
    *pos += number_of_62s;
    let last_run = *actual_end_ptr;

    let remaining_run = if (QOI_OP_RUN..QOI_OP_RGB).contains(&last_run) {
        *pos += 1;
        length_from_op_run(last_run)
    } else {
        0
    };
    (number_of_62s * 62) + remaining_run
}

#[inline(always)]
unsafe fn load_run(length: usize, prev_ptr: *const [u8; 4], output_ptr: *mut [u8; 4]) {
    asm!(
        "cld",
        "mov eax, [{prev}]",
        "rep stosd",
        prev = in(reg) prev_ptr,
        in("rcx") length + 1,
        in("rdi") output_ptr,
        out("eax") _,
    )
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
unsafe fn load_one_luma(op_ptr: *const u16, prev_ptr: *const [u8; 4], output_ptr: *mut [u8; 4]) {
    asm!(
        // load the pixel as two words
        "mov        {green_red:x},  [{prev}]",
        "mov        {alpha_blue:x}, [{prev} + 2]",

        // load the OP_LUMA as one word and copy 2nd byte out
        "mov        {db_dg:x},      [{op_ptr}]",
        "mov        {dr:l},         {db_dg:h}", // the byte contains both blue and red

        // isolate the real values of [dr - dg + 8, dg + 32, db - dg + 8]
        "shr        {dr:l},         4",
        "and        {db_dg:x},      3903",      // 0x0f3f

        // remove bias from deltas and apply them to the previous pixel
        // abuse the hell out of the add circuit's reciprocal throughput of 0.25
        "sub        {db_dg:l},      40",        // push dg's bias over to apply to dr and db
        "add        {db_dg:h},      {db_dg:l}",
        "add        {dr:l},         {db_dg:l}",
        "add        {db_dg:l},      8",         // correct the over-bias above
        "add        {alpha_blue:l}, {db_dg:h}",
        "add        {green_red:l},  {dr:l}",
        "add        {green_red:h},  {db_dg:l}",

        // write output directly to buffer
        "mov        [{output}],     {green_red:x}",
        "mov        [{output} + 2], {alpha_blue:x}",

        op_ptr      = in(reg)        op_ptr,
        prev        = in(reg)        prev_ptr,
        output      = in(reg)        output_ptr,

        db_dg       = out(reg_abcd)  _,
        dr          = out(reg_abcd)  _,
        alpha_blue  = out(reg_abcd)  _,
        green_red   = out(reg_abcd)  _,

        options(nostack)
    )
}
