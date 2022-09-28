use crate::alloc::vec::Vec;
use crate::common::{QOI_OP_DIFF, QOI_OP_INDEX, QOI_OP_LUMA, QOI_OP_RGB, QOI_OP_RGBA, QOI_OP_RUN};
use core::arch::asm;
use core::mem::replace;

// ed is the encoding duration
pub(crate) struct EncodeContext<'ed> {
    input_buffer: &'ed [u32],
    output_buffer: &'ed mut Vec<u8>,
    hashes: Vec<u8>,
    hash_index_array: [u32; 64],
    input_position: usize,
    previous_pixel: &'ed u32,
}

impl<'ed> EncodeContext<'ed> {
    pub fn new(input_buffer: &'ed [u32], output_buffer: &'ed mut Vec<u8>, hashes: Vec<u8>) -> Self {
        Self {
            input_buffer,
            output_buffer,
            hashes,

            hash_index_array: [0u32; 64],
            input_position: 0,
            previous_pixel: &0xff000000u32,
        }
    }

    pub fn get_pos(&self) -> usize {
        return self.input_position;
    }

    pub fn get_pixel(&self) -> u32 {
        return self.input_buffer[self.input_position];
    }

    unsafe fn get_pixel_ptr(&self) -> *const u32 {
        return self.input_buffer.as_ptr().add(self.input_position);
    }

    unsafe fn get_output_ptr(&mut self) -> *mut u8 {
        return self
            .output_buffer
            .as_mut_ptr()
            .add(self.output_buffer.len());
    }

    #[inline(always)]
    pub fn get_hash(&self) -> u8 {
        return self.hashes[self.input_position];
    }

    #[inline(always)]
    pub fn get_previous_pixel(&self) -> u32 {
        return *self.previous_pixel;
    }

    pub fn update_pos(&mut self) {
        self.previous_pixel = &self.input_buffer[self.input_position];
        self.input_position += 1;
    }

    pub fn swap_hash(&mut self) -> u32 {
        let pixel = self.get_pixel();
        return replace(&mut self.hash_index_array[self.get_hash() as usize], pixel);
    }

    pub fn write_rgba(&mut self) -> () {
        self.output_buffer.push(QOI_OP_RGBA);
        self.output_buffer.extend(self.get_pixel().to_ne_bytes());
    }

    pub fn write_rgb(&mut self) -> () {
        self.output_buffer
            .extend(((self.get_pixel() << 8) | QOI_OP_RGB as u32).to_ne_bytes());
    }

    pub fn write_hash_index(&mut self) -> () {
        self.output_buffer
            .push(QOI_OP_INDEX | self.hashes[self.input_position]);
    }

    pub fn write_diff(&mut self, deltas: [u8; 4]) {
        self.output_buffer
            .push(QOI_OP_DIFF | deltas[0] << 4 | deltas[1] << 2 | deltas[2]);
    }

    pub fn write_luma(&mut self, dg: u8, dr_dg: u8, db_dg: u8) {
        self.output_buffer.push(QOI_OP_LUMA | dg);
        self.output_buffer.push((dr_dg << 4) | db_dg);
    }

    #[inline(always)]
    pub fn write_run(&mut self, max_runs: usize, remainder: usize) {
        let rem_op = QOI_OP_RUN | ((remainder as u8).wrapping_sub(1) & !QOI_OP_RUN);
        let additional = max_runs;

        if max_runs > 0 {
            self.output_buffer.reserve_exact(additional);
            unsafe {
                asm!(
                    "cld",
                    "rep stosb",
                    inout("rcx") additional => _,
                    inout("rdi") self.get_output_ptr() => _,
                    in("al") 0xfdu8,
                );
                self.output_buffer
                    .set_len(self.output_buffer.len() + additional);
            }
            if remainder != 0 {
                self.output_buffer.push(rem_op);
            }
        } else {
            self.output_buffer.push(rem_op);
        }
    }

    #[inline(always)]
    pub fn find_run_length_at_current_position(&mut self) -> (usize, usize) {
        let total_run_length = unsafe {
            let start_ptr = self.get_pixel_ptr();
            let mut end_ptr: *const u32;

            asm!(
                "cld",
                "mov eax, [rdi]",
                "repe scasd",
                inout("rdi") start_ptr => end_ptr,
                inout("rcx") (self.input_buffer.len() - self.input_position) + 1 => _,
                out("eax") _
            );

            let actual_end_ptr = end_ptr.sub(1);

            actual_end_ptr.offset_from(start_ptr)
        };

        debug_assert!(total_run_length > 0);
        let total_run_length = total_run_length as usize;
        self.input_position += total_run_length;

        let n_max_runs = total_run_length / 62;
        let remaining_run = total_run_length % 62;

        return (n_max_runs, remaining_run);
    }
}
