#![no_std]
use super::super::hashing::{HashIndexedArray, Hashing};
use crate::common::{QOI_OP_RGB, QOI_OP_RGBA, QOI_OP_RUN};
use core::arch::asm;

const RGBA_CHA_CHA: u128 = 0x80808080_0d0c0b0a_08070605_03020100_u128;
const RGB_LAST_ALPHA_SWITCHEROO: u128 = 0x80808080_80808080_80808080_05020100_u128;
const DIFF_MUL_DUP: u32 = 0x01004010_u32;
const DIFF_MASK: u32 = 0x03030303_u32;

// ed is the encoding duration

#[derive(Debug)]
pub(crate) struct DecodeContext<'ed> {
    input_buffer: &'ed Vec<u8>,
    pub(crate) output_buffer: &'ed mut Vec<[u8; 4]>,
    last_hash_update: usize,
    hash_index_array: HashIndexedArray,
    input_position: usize,
    pub(crate) previous_pixel: *const u32,
}

impl<'ed> DecodeContext<'ed> {
    pub fn new(input_buffer: &'ed Vec<u8>, output_buffer: &'ed mut Vec<[u8; 4]>) -> Self {
        Self {
            input_buffer,
            output_buffer,

            last_hash_update: 0,
            hash_index_array: HashIndexedArray::new(),
            input_position: 14,
            previous_pixel: &0xff000000u32,
        }
    }

    unsafe fn get_output_ptr(&mut self) -> *mut [u8; 4] {
        return self
            .output_buffer
            .as_mut_ptr()
            .add(self.output_buffer.len());
    }

    unsafe fn register_more_output(&mut self, additional: usize) {
        self.output_buffer
            .set_len(self.output_buffer.len() + additional);
    }

    /// An easily inlinable function that expands the OP_DIFF byte into an array
    /// 0b_01_dr_dg_db_u8 -> [0b000000dr_u8, 0b000000dg_u8, 0b000000db_u8, 0u8]
    const fn op_diff_expand222(x: u8) -> u32 {
        // thanks for this function, https://github.com/adrianparvino ! :)
        let y = (x as u32) * DIFF_MUL_DUP;
        (y & DIFF_MASK) >> 8
    }

    pub(crate) const fn is_run(byte: u8) -> bool {
        QOI_OP_RUN <= byte && byte < QOI_OP_RGB
    }

    fn update_hia(&mut self) {
        self.hash_index_array
            .update(&self.output_buffer[self.last_hash_update..]);
    }

    pub(crate) fn hia_push_prev(&mut self) {
        self.hash_index_array
            .push(unsafe { *(self.previous_pixel as *const [u8; 4]) });
    }

    #[inline(always)]
    pub(crate) fn pos(&self) -> usize {
        return self.input_position;
    }

    #[inline(always)]
    pub(crate) fn get_byte(&self) -> u8 {
        return self.input_buffer[self.input_position];
    }

    #[inline(always)]
    fn get_byte_ref(&self) -> &u8 {
        return &self.input_buffer[self.input_position];
    }

    #[inline(always)]
    fn get_byte_offset(&self, offset: usize) -> u8 {
        return self.input_buffer[self.input_position + offset];
    }

    pub(crate) unsafe fn update_previous_ptr(&mut self) {
        self.previous_pixel = self.get_output_ptr() as *const u32;
    }

    pub(crate) unsafe fn load_some_rgba(&mut self) {
        self.input_position += 1;
        // look ahead to see if there are multiple
        if self.get_byte_offset(4) == QOI_OP_RGBA {
            // whether there are two or three, it still helps to move them together
            let theres_three_actually = (self.get_byte_offset(9) == QOI_OP_RGBA) as usize;
            let n_added = 2 + theres_three_actually;

            self.load_three_rgba();
            self.register_more_output(n_added);
            self.previous_pixel = (self.get_output_ptr() as *const u32).sub(1);

            self.input_position += 4 + 5 + (theres_three_actually * 5);
        } else {
            self.load_one_rgba();
            self.update_previous_ptr();
            self.register_more_output(1);
            self.input_position += 4;
        }
    }

    #[inline(always)]
    unsafe fn load_three_rgba(&mut self) {
        asm!(
            // from points to the first R, so the contents of staging will be either
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
        *self.get_output_ptr() = *(self.get_byte_ref() as *const u8 as *const [u8; 4]);
    }

    pub(crate) unsafe fn load_one_rgb(&mut self) {
        self.input_position += 1;
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

            rgbx        = in(reg)       self.get_byte_ref(),
            prev        = in(reg)       self.previous_pixel,
            output      = in(reg)       self.get_output_ptr(),
            shuffle_ptr = in(reg)       &RGB_LAST_ALPHA_SWITCHEROO,

            shuffler    = out(xmm_reg)  _,
            staging     = out(xmm_reg)  _,

            options(nostack, preserves_flags)
        );
        self.update_previous_ptr();
        self.register_more_output(1);
        self.input_position += 3;
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
        let op_and_dg = self.get_byte();
        let byte_2 = self.get_byte_offset(1);
        let dg_m8 = op_and_dg.wrapping_sub(0b10000000_u8 + 40u8);
        let [pr, pg, pb, pa] = *(self.previous_pixel as *const [u8; 4]);

        *self.get_output_ptr() = [
            pr.wrapping_add((byte_2 >> 4).wrapping_add(dg_m8)),
            pg.wrapping_add(dg_m8.wrapping_add(8u8)),
            pb.wrapping_add((byte_2 & 0xf).wrapping_add(dg_m8)),
            pa,
        ];

        self.update_previous_ptr();
        self.register_more_output(1);
        self.input_position += 2;
    }

    #[inline(always)]
    const fn length_from_op_run(op_run: u8) -> usize {
        return (op_run & !QOI_OP_RUN) as usize + 1;
    }

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

        return (number_of_62s * 62) + remaining_run;
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
