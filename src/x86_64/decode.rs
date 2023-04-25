use alloc::vec::Vec;
use core::arch::asm;

use super::hashing::Hashing;
use crate::common::{
    QOIHeader, END_8, QOI_OP_DIFF, QOI_OP_INDEX, QOI_OP_LUMA, QOI_OP_RGB, QOI_OP_RGBA, QOI_OP_RUN,
    RGBA,
};
use core::hint::unreachable_unchecked;

const RGBA_CHA_CHA: u128 = 0x80808080_0d0c0b0a_08070605_03020100_u128;
const DIFF_MUL_DUP: u32 = 0x01004010_u32;
const DIFF_MASK: u32 = 0x03030303_u32;

// ed is the encoding duration
pub(crate) struct DecodeContext<'ed> {
    input_buffer: &'ed Vec<u8>,
    pub(crate) output_buffer: &'ed mut Vec<RGBA>,
    last_hash_update: usize,
    hash_index_array: [RGBA; 64],
    input_position: usize,
    pub(crate) previous_pixel: *const RGBA,
}

impl<'ed> DecodeContext<'ed> {
    pub fn new(input_buffer: &'ed Vec<u8>, output_buffer: &'ed mut Vec<RGBA>) -> Self {
        Self {
            input_buffer,
            output_buffer,

            last_hash_update: 0,
            hash_index_array: [0u32; 64],
            input_position: 14,
            previous_pixel: &0xff000000u32,
        }
    }

    unsafe fn get_output_ptr(&mut self) -> *mut RGBA {
        self.output_buffer
            .as_mut_ptr()
            .add(self.output_buffer.len())
    }

    unsafe fn register_more_output(&mut self, additional: usize) {
        self.output_buffer
            .set_len(self.output_buffer.len() + additional);
    }

    /// An easily inlinable function that expands the OP_DIFF byte into an array
    /// 0b_01_dr_dg_db_u8 -> [0b000000dr_u8, 0b000000dg_u8, 0b000000db_u8, 0u8]
    const fn op_diff_expand222(x: u8) -> RGBA {
        // thanks for this function, https://github.com/adrianparvino ! :)
        let y = (x as RGBA) * DIFF_MUL_DUP;
        (y & DIFF_MASK) >> 8
    }

    pub(crate) const fn is_run(byte: u8) -> bool {
        QOI_OP_RUN <= byte && byte < QOI_OP_RGB
    }

    fn update_hia(&mut self) {
        let untouched_pixels = &self.output_buffer[self.last_hash_update..];
        self.hash_index_array.update(untouched_pixels);
    }

    #[inline(always)]
    pub(crate) fn pos(&self) -> usize {
        self.input_position
    }

    #[inline(always)]
    pub(crate) fn get_byte(&self) -> u8 {
        self.input_buffer[self.input_position]
    }

    #[inline(always)]
    fn get_byte_ref(&self) -> &u8 {
        &self.input_buffer[self.input_position]
    }

    #[inline(always)]
    fn get_byte_with_offset(&self, offset: usize) -> u8 {
        self.input_buffer[self.input_position + offset]
    }

    #[inline(always)]
    pub(crate) unsafe fn update_previous_ptr(&mut self) {
        self.previous_pixel = self.get_output_ptr() as *const u32;
    }

    pub(crate) unsafe fn load_some_rgba(&mut self) {
        self.input_position += 1;
        // look ahead to see if there are multiple
        if self.get_byte_with_offset(4) == QOI_OP_RGBA {
            // whether there are two or three, it still helps to move them together
            let theres_three_actually = self.get_byte_with_offset(9) == QOI_OP_RGBA;
            let n_added = 2 + theres_three_actually as usize;

            self.load_three_rgba();
            self.register_more_output(n_added);
            self.previous_pixel = (self.get_output_ptr() as *const RGBA).sub(1);

            self.input_position += (5 * n_added) - 1;
        } else {
            self.load_one_rgba();
            self.update_previous_ptr();
            self.register_more_output(1);
            self.input_position += 4;
        }
    }

    /// This function takes a string of two pr three encoded RGBA OPs and uses a shuffle to remove
    /// the OP byte from each of them and write them directly to the output pixel buffer.
    /// As a minor side effect, leftovers are also written to the end of the buffer but will be
    /// overwritten by the next write to it because the pointer only gets incremented based on the
    /// number of valid RGBAs added.
    #[inline(always)]
    unsafe fn load_three_rgba(&mut self) {
        asm!(
        // in_ptr points to the first R, so the contents of staging will be either
        // [RGBA ORGB AORG BAXX] or [RGBA ORGB AXXX XXXX]
        "movdqu     {staging},      [{in_ptr}]",
        "movdqu     {shuffler},     [{shuffle_ptr}]",
        "pshufb     {staging},      {shuffler}",
        "movdqu     [{output_ptr}], {staging}",

        in_ptr      = in(reg)       self.get_byte_ref(),
        output_ptr  = in(reg)       self.get_output_ptr(),
        shuffle_ptr = in(reg)       &RGBA_CHA_CHA,

        staging     = out(xmm_reg)  _,
        shuffler    = out(xmm_reg)  _,

        options(nostack, preserves_flags)
        );
    }

    #[inline(always)]
    unsafe fn load_one_rgba(&mut self) {
        *self.get_output_ptr() = *(self.get_byte_ref() as *const u8 as *const RGBA);
    }

    pub(crate) unsafe fn load_one_rgb(&mut self) {
        asm!(
        // get the red, green, and blue, with the op also in the lowest byte
        "mov       {staging:e},      [{orgb}]",
        // overwrite the op with the alpha of the previous pixel, such that the staging is now argb
        "mov       {staging:l},     [{prev}+3]",
        // move the a so that it is rgba
        "ror        {staging:e}, 8",
        // put the resulting pixel in to the output buffer
        "mov        [{output}],     {staging:e}",

        orgb        = in(reg)       self.get_byte_ref(),
        prev        = in(reg)       self.previous_pixel,
        output      = in(reg)       self.get_output_ptr(),
        staging     = out(reg)  _,

        options(nostack, preserves_flags)
        );
        self.update_previous_ptr();
        self.register_more_output(1);
        self.input_position += 4;
    }

    pub(crate) unsafe fn load_diff(&mut self) {
        let diff = Self::op_diff_expand222(self.get_byte());
        asm!(
        "movd       {pixel_xmm},    [{prev}]",
        "movd       {diff_xmm},     {diff:e}",
        "movd       {bias_xmm},     {bias:e}",

        "paddb      {pixel_xmm},    {diff_xmm}",
        "psubb      {pixel_xmm},    {bias_xmm}",

        "movd       [{output}],     {pixel_xmm}",

        prev        = in(reg)       self.previous_pixel,
        diff        = in(reg)       diff,
        bias        = in(reg)       0x00020202_u32,
        output      = in(reg)       self.get_output_ptr(),

        pixel_xmm   = out(xmm_reg)  _,
        diff_xmm    = out(xmm_reg)  _,
        bias_xmm    = out(xmm_reg)  _,

        options(nostack, preserves_flags)
        );

        self.update_previous_ptr();
        self.register_more_output(1);
        self.input_position += 1;
    }

    pub(crate) unsafe fn load_one_luma(&mut self) {
        asm!(
        " # LLVM-MCA-BEGIN luma",

        "movzx  {staging:e},    word ptr [{op_ptr}]",   // get op and second byte from memory
        "movzx  {green:e},      {staging:l}",           // copy out green

        // copy the two bytes 20 bits up, cutting off the duplicated dr
        // in the nibble views below, R is (dr - dg + 8), B is (db - dg + 8), G is the low nibble of (dg + 32),
        // O is OP_luma and the two top bits of (dg + 32)
        "imul   {staging:e},    {staging:e},    1048577",   // staging: 0xBOG0RBOG
        // shift the result so that the red, green, and blue are arranged like RGB normally is
        "shr    {staging:e},    12",                        // staging: 0x000BOG0R
        "mov    {staging:h},    8",                         // staging: 0x000B080R

        "sub    {green:l},  168",               // offset the delta green by negative 40 to (dr - 8)
        "imul   {green:e},  {green:e},  65793", // duplicate the delta green over the first three bytes

        "movd   {greens},   {green:e}",
        "movd   {deltas},   {staging:e}",
        // add the delta greens and offsets properly
        // (dr - dg + 8) + (dg - 8) = (dr), (8) + (dg - 8) = (dg), (db - dg + 8) + (dg - 8) = (db)
        // so in summary we now have unbiased, raw deltas
        "paddb  {deltas},   {greens}",
        "movd   {pixel},    [{previous_ptr}]",
        "paddb  {pixel},    {deltas}",
        "movd   [{out}],    {pixel}",
        " # LLVM-MCA-END luma",

        op_ptr = in(reg) self.get_byte_ref(),
        previous_ptr = in(reg) self.previous_pixel,
        out = in(reg) self.get_output_ptr(),

        staging = out(reg_abcd) _,
        green = out(reg) _,

        greens = lateout(xmm_reg) _,
        deltas = out(xmm_reg) _,
        pixel = out(xmm_reg) _,

        );

        self.update_previous_ptr();
        self.register_more_output(1);
        self.input_position += 2;
    }

    #[inline(always)]
    const fn length_from_op_run(op_run: u8) -> usize {
        (op_run & !QOI_OP_RUN) as usize + 1
    }

    #[inline(always)]
    unsafe fn scan_run_length(&mut self) -> usize {
        let start_ptr = self.get_byte_ref() as *const u8;
        let mut end_ptr: *const u8;

        asm!(
        "cld",
        "repe scasb",
        in("al") 0xfdu8,
        inout("rdi") start_ptr => end_ptr,
        inout("rcx") (self.input_buffer.len() - self.input_position) + 1 => _
        );

        let actual_end_ptr = end_ptr.sub(1);
        let number_of_62s = actual_end_ptr as usize - start_ptr as usize;
        self.input_position += number_of_62s;
        let last_run = *actual_end_ptr;

        let remaining_run = if Self::is_run(last_run) {
            self.input_position += 1;
            Self::length_from_op_run(last_run)
        } else {
            0
        };

        (number_of_62s * 62) + remaining_run
    }

    #[inline(always)]
    unsafe fn store_run(&mut self, length: usize) {
        asm!(
        "cld",
        "rep stosd",
        in("rcx") length + 1,
        in("rdi") self.get_output_ptr(),
        in("eax") *self.previous_pixel,
        )
    }

    pub(crate) unsafe fn load_run(&mut self) {
        self.update_hia();
        let run_length = self.scan_run_length();
        self.store_run(run_length);
        self.register_more_output(run_length);
        self.last_hash_update = self.output_buffer.len();
    }

    pub(crate) unsafe fn load_index(&mut self) {
        self.update_hia();
        self.last_hash_update = self.output_buffer.len();
        let index = self.get_byte() & 0x3f;
        self.update_previous_ptr();
        self.output_buffer.push(self.hash_index_array.fetch(index));
        self.input_position += 1;
    }
}

#[inline(never)]
pub fn decode(input: &Vec<u8>, output: &mut Vec<RGBA>) -> Result<(), (usize, usize)> {
    let header = QOIHeader::from(input.as_slice());
    output.reserve_exact(header.image_size());
    let mut ctx: DecodeContext = DecodeContext::new(input, output);

    let len: usize = input.len() - 8;

    // if the first op is a run, black ends up not in the HIA because of the hash-skipping behaviour
    if DecodeContext::is_run(ctx.get_byte()) {
        // this fixes that
        ctx.hash_index_array
            .update([unsafe { *ctx.previous_pixel }].as_ref());
    }

    while ctx.input_position < len {
        let next_op: u8 = ctx.get_byte();

        match next_op {
            QOI_OP_RGBA => unsafe {
                ctx.load_some_rgba();
            },
            QOI_OP_RGB => unsafe {
                ctx.load_one_rgb();
            },
            // it turns out that the compiler can make this into a LUT without me manually doing so
            _ => match next_op & 0b11000000 {
                QOI_OP_DIFF => unsafe {
                    ctx.load_diff();
                },
                QOI_OP_LUMA => unsafe {
                    ctx.load_one_luma();
                },
                QOI_OP_RUN => unsafe {
                    ctx.load_run();
                },
                QOI_OP_INDEX => unsafe {
                    ctx.load_index();
                },
                _ => unsafe { unreachable_unchecked() },
            },
        } // end match 8-bit
    } // end loop

    let pos = ctx.pos();

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
