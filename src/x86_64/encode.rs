use super::HASH_RGBA_MANY;
use crate::common::{
    QOIHeader, END_8, HASH, MAGIC_QOIF, QOI_OP_DIFF, QOI_OP_INDEX, QOI_OP_LUMA, QOI_OP_RGB,
    QOI_OP_RGBA, QOI_OP_RUN, RGBA,
};
use alloc::vec::Vec;
use core::arch::asm;
use core::mem::replace;

// ed is the encoding duration
pub(crate) struct EncodeContext<'ed> {
    pixel_count: usize,
    input_pixels: &'ed Vec<RGBA>,
    output_bytes: &'ed mut Vec<u8>,
    hashes: Vec<HASH>,
    hash_index_array: [RGBA; 64],
    position: usize,
    previous_pixel: &'ed RGBA,
}

impl<'ed> EncodeContext<'ed> {
    pub fn new(input_pixels: &'ed Vec<RGBA>, output_bytes: &'ed mut Vec<u8>) -> Self {
        let pixel_count = input_pixels.len();
        Self {
            pixel_count,
            input_pixels,
            output_bytes,
            hashes: Vec::with_capacity(pixel_count),
            hash_index_array: [0u32; 64],
            position: 0,
            previous_pixel: &0xff000000u32,
        }
    }

    #[inline(never)]
    pub fn initialize_hashes(&mut self) {
        unsafe {
            let chunk_count = self.pixel_count / HASH_RGBA_MANY.hash_chunk_size();
            // print!("The hash buffer starts at {:?} which is ", self.hashes.as_ptr());
            // if self.hashes.as_ptr().align_offset(32) > 0 {
            //     print!("not ");
            // }
            // println!("aligned to 32 bytes");
            HASH_RGBA_MANY.hash_chunks(
                self.input_pixels.as_ptr(),
                self.hashes.as_mut_ptr(),
                chunk_count,
            );
            self.hashes.set_len(self.pixel_count);
        }
    }

    pub fn get_pixel(&self) -> RGBA {
        self.input_pixels[self.position]
    }

    unsafe fn get_pixel_ptr(&self) -> *const RGBA {
        self.input_pixels.as_ptr().add(self.position)
    }

    unsafe fn get_output_ptr(&mut self) -> *mut u8 {
        self.output_bytes.as_mut_ptr().add(self.output_bytes.len())
    }

    #[inline(always)]
    pub fn get_hash(&self) -> HASH {
        self.hashes[self.position]
    }

    #[inline(always)]
    pub fn get_previous_pixel(&self) -> RGBA {
        *self.previous_pixel
    }

    pub fn update_pos(&mut self) {
        self.previous_pixel = &self.input_pixels[self.position];
        self.position += 1;
    }

    pub fn swap_hash(&mut self) -> RGBA {
        let pixel = self.get_pixel();
        replace(&mut self.hash_index_array[self.get_hash() as usize], pixel)
    }

    pub fn write_rgba(&mut self) {
        self.output_bytes.push(QOI_OP_RGBA);
        self.output_bytes.extend(self.get_pixel().to_ne_bytes());
    }

    pub fn write_rgb(&mut self) {
        self.output_bytes
            .extend(((self.get_pixel() << 8) | QOI_OP_RGB as RGBA).to_ne_bytes());
    }

    pub fn write_hash_index(&mut self) {
        self.output_bytes
            .push(QOI_OP_INDEX | self.hashes[self.position]);
    }

    pub fn write_diff(&mut self, deltas: [u8; 4]) {
        self.output_bytes
            .push(QOI_OP_DIFF | deltas[0] << 4 | deltas[1] << 2 | deltas[2]);
    }

    pub fn write_luma(&mut self, dg: u8, dr_dg: u8, db_dg: u8) {
        self.output_bytes.push(QOI_OP_LUMA | dg);
        self.output_bytes.push((dr_dg << 4) | db_dg);
    }

    #[inline(always)]
    pub fn write_run(&mut self, max_runs: usize, remainder: usize) {
        let rem_op = QOI_OP_RUN | ((remainder as u8).wrapping_sub(1) & !QOI_OP_RUN);
        let additional = max_runs;

        if max_runs > 0 {
            self.output_bytes.reserve_exact(additional);
            unsafe {
                asm!(
                "cld",
                "rep stosb",
                inout("rcx") additional => _,
                inout("rdi") self.get_output_ptr() => _,
                in("al") 0xfdu8,
                );
                self.output_bytes
                    .set_len(self.output_bytes.len() + additional);
            }
            if remainder != 0 {
                self.output_bytes.push(rem_op);
            }
        } else {
            self.output_bytes.push(rem_op);
        }
    }

    #[inline(always)]
    pub fn find_run_length_at_current_position(&mut self) -> (usize, usize) {
        let total_run_length = unsafe {
            let start_ptr = self.get_pixel_ptr();
            let mut end_ptr: *const RGBA;

            asm!(
                "cld",
                "mov eax, [rdi]",
                "repe scasd",
                inout("rdi") start_ptr => end_ptr,
                inout("rcx") (self.input_pixels.len() - self.position) + 1 => _,
                out("eax") _
            );

            let actual_end_ptr = end_ptr.sub(1);

            actual_end_ptr.offset_from(start_ptr)
        };

        debug_assert!(total_run_length > 0);
        let total_run_length = total_run_length as usize;
        self.position += total_run_length;

        let n_max_runs = total_run_length / 62;
        let remaining_run = total_run_length % 62;

        (n_max_runs, remaining_run)
    }
}

pub fn encode(
    input_pixels: &Vec<RGBA>,
    output_bytes: &mut Vec<u8>,
    metadata: QOIHeader,
) -> Result<(), (usize, usize)> {
    output_bytes.extend(MAGIC_QOIF);
    output_bytes.extend(metadata.to_bytes());

    let pixel_count = metadata.image_size();
    assert_eq!(input_pixels.len(), pixel_count);

    // Create encoding context
    let ctx = EncodeContext::new(input_pixels, output_bytes);

    encode_pixels(ctx)?;
    output_bytes.extend(END_8);
    Ok(())
}

#[inline(never)]
fn encode_pixels(mut encode_context: EncodeContext) -> Result<(), (usize, usize)> {
    encode_context.initialize_hashes();

    while encode_context.position < encode_context.pixel_count {
        let pixel = encode_context.get_pixel();
        let pixel_of_same_hash = encode_context.swap_hash();

        if pixel == encode_context.get_previous_pixel() {
            let (max_runs, last_run) = encode_context.find_run_length_at_current_position();
            encode_context.write_run(max_runs, last_run);

            continue;
        }

        if pixel == pixel_of_same_hash {
            encode_context.write_hash_index();
        } else if (pixel & 0xff000000) != (encode_context.get_previous_pixel() & 0xff000000) {
            encode_context.write_rgba();
        } else {
            let mut delta_pixel: u32;

            // pre-biased because that's easier to compare anyway
            let mut delta_pixel_bias2: u32;
            unsafe {
                asm!(
                "movd {delta_px_xmm}, {pixel:e}",
                "movd {last_pixel_xmm}, {last_pixel:e}",
                "psubb {delta_px_xmm}, {last_pixel_xmm}",
                "movd {delta_px:e}, {delta_px_xmm}",

                "movd {bias_2_xmm}, {bias:e}",
                "paddb {bias_2_xmm}, {delta_px_xmm}",
                "movd {delta_px_b2:e}, {bias_2_xmm}",

                // input
                pixel = in(reg) pixel,
                last_pixel = in(reg) encode_context.get_previous_pixel(),
                bias = in(reg) 0x00020202,

                // output ptrs
                delta_px = out(reg) delta_pixel,
                delta_px_b2 = out(reg) delta_pixel_bias2,

                // automatically assigned xmm regs
                delta_px_xmm = out(xmm_reg) _,
                last_pixel_xmm = lateout(xmm_reg) _,
                bias_2_xmm = out(xmm_reg) _,

                options(preserves_flags, nostack)
                );
            }

            let delta_pixel = bytemuck::cast::<u32, [u8; 4]>(delta_pixel);
            let delta_pixel_bias2 = bytemuck::cast::<u32, [u8; 4]>(delta_pixel_bias2);

            if delta_pixel_bias2[0] < 4u8
                && delta_pixel_bias2[1] < 4u8
                && delta_pixel_bias2[2] < 4u8
            {
                encode_context.write_diff(delta_pixel_bias2);
            } else {
                let [dr, dg, db, _] = delta_pixel;
                let dg_bias32 = dg.wrapping_add(32u8);
                let dr_dg_bias8 = dr.wrapping_sub(dg).wrapping_add(8u8);
                let db_dg_bias8 = db.wrapping_sub(dg).wrapping_add(8u8);

                if dg_bias32 < 64 && dr_dg_bias8 < 16 && db_dg_bias8 < 16 {
                    encode_context.write_luma(dg_bias32, dr_dg_bias8, db_dg_bias8);
                } else {
                    encode_context.write_rgb();
                }
            }
        }

        encode_context.update_pos();
    }
    let last_pos = encode_context.position;
    if last_pos == encode_context.pixel_count {
        Ok(())
    } else {
        Err((last_pos, encode_context.pixel_count))
    }
}
