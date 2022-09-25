static MOD64MASK: u64 = 0x003f003f003f003fu64;
static HASHING_NUMS_RGBA: u64 = 0x0b0705030b070503u64;

use crate::alloc::vec::Vec;
pub use crate::{HashIndexedArray, Hashing};
use core::arch::asm;

impl Hashing for HashIndexedArray {
    fn update(&mut self, pixel_feed: &[[u8; 4]]) {
        let len = pixel_feed.len();
        if len == 0 {
            return;
        } else if len == 1 {
            let pixel = pixel_feed[0];

            unsafe {
                asm!(
                    "mov [{origin} + 4*{hash}], {pixel:e}",
                    origin = in(reg) &self.indices_array,
                    hash = in(reg) hash_rgba(pixel),
                    pixel = in(reg) u32::from_ne_bytes(pixel)
                )
            }
        } else {
            let bytes = bytemuck::cast_vec::<[u8; 4], u8>(Vec::from(pixel_feed));
            let hashes = hashes_rgba(&bytes, len);
            for i in 0..hashes.len() {
                unsafe {
                    *self
                        .indices_array
                        .get_unchecked_mut(*hashes.get_unchecked(i) as usize) =
                        *pixel_feed.get_unchecked(i);
                }
            }
        }
    }

    fn fetch(&mut self, hash: u8) -> [u8; 4] {
        self.indices_array[hash as usize]
    }

    fn push(&mut self, pixel: [u8; 4]) -> ([u8; 4], u8) {
        let hash = hash_rgba(pixel);
        let pixel2 = core::mem::replace(&mut self.indices_array[hash as usize], pixel);
        (pixel2, hash as u8)
    }

    fn new() -> Self {
        Self {
            indices_array: [[0u8; 4]; 64],
        }
    }
}

pub fn hashes_rgba(bytes: &Vec<u8>, count: usize) -> Vec<u8> {
    // this wraps the "unsafe" enclosed function to make the most efficient use of SIMD
    if count <= 8 {
        unsafe { simd_hashes_lt8(bytes, count) }
    } else {
        unsafe { simd_hashes_many(bytes, count) }
    }
}

#[inline(always)] // because it's wrapped by the above function, a nested call isn't useful
unsafe fn simd_hashes_many(bytes: &Vec<u8>, count: usize) -> Vec<u8> {
    let safe_iterations = count / 16 + 1;
    let safe_alloc_bytes = safe_iterations * 16;
    // Allocate 1-16 bytes extra for the hashes vector, so that writing the final xmm doesn't
    // overwrite anything that comes after it and corrupt anything. The capacity should not change,
    // but the size should be set after writing everything.
    let mut hashes: Vec<u8> = Vec::with_capacity(safe_alloc_bytes);

    let mut pixels_ptr = bytes.as_ptr();
    let mut hashes_ptr = hashes.as_mut_ptr();

    // reserve xmm0 and xmm1 for quick access of the hashing numbers and mod mask
    asm!(
        "movddup        xmm10,      [{multipliers_ptr}]",
        "movddup        xmm11,      [{mask_ptr}]",

        multipliers_ptr = in(reg)   &HASHING_NUMS_RGBA,
        mask_ptr        = in(reg)   &MOD64MASK,

        out("xmm10") _,
        out("xmm11") _,

        options(nostack, preserves_flags, readonly)
    );

    for _ in 0..safe_iterations {
        asm!(
            // load 16 pixels into four xmm registers
            "movdqu     {a},            [{pixels_ptr}]",        // get b from chunk
            "movdqu     {b},            [{pixels_ptr} + 16]",   // get b from chunk
            "movdqu     {c},            [{pixels_ptr} + 32]",   // get c from chunk
            "movdqu     {d},            [{pixels_ptr} + 48]",   // get d from chunk
            "lea        {pixels_ptr},   [{pixels_ptr} + 64]",

            // multiply and add all pairs of pixel channels simultaneously
            "pmaddubsw  {a},            xmm10",
            "pmaddubsw  {b},            xmm10",
            "pmaddubsw  {c},            xmm10",
            "pmaddubsw  {d},            xmm10",
            // horizontally add the channel pairs into final sums
            "phaddw     {a},            {b}",
            "phaddw     {c},            {d}",
            // cheating % 64
            "pand       {a},            xmm11", // a is now the hashes of the pixels originally in a and b
            "pand       {c},            xmm11", // c is now the hashes of the pixels originally in c and d

            "packuswb   {a},            {c}",   // a becomes the final 16 hashes in byte form
            "movntdq    [{hashes_ptr}], {a}",   // put a into list of hash results
            "lea        {hashes_ptr},   [{hashes_ptr} + 16]",

            pixels_ptr  = inout(reg)    pixels_ptr,
            hashes_ptr  = inout(reg)    hashes_ptr,

            // probably best to let these be assigned by the assembler
            a           = out(xmm_reg)  _,
            b           = out(xmm_reg)  _,
            c           = out(xmm_reg)  _,
            d           = out(xmm_reg)  _,
                          out("xmm10")  _,      // reserved for hashing numbers
                          out("xmm11")  _,      // reserved for mod 64 mask

            options(preserves_flags, nostack)
        );
    }

    asm!("sfence"); // to tell other cores where all that movntdq'd stuff came from, which shouldn't affect anything
    hashes.set_len(count);

    return hashes;
}
#[inline(always)]
/// A stripped down SIMD hashing for pixel counts between 2 and 8 (inclusive)
unsafe fn simd_hashes_lt8(bytes: &Vec<u8>, count: usize) -> Vec<u8> {
    let mut output: Vec<u8> = Vec::with_capacity(8);

    asm!(
        "movddup        {multipliers},      [{multipliers_ptr}]",
        "movddup        {mod_64_mask},      [{mask_ptr}]",

        // load up to 8 pixels into two xmm registers
        "movdqu         {a},                [{in_ptr}]",        // get a from chunk
        "movdqu         {b},                [{in_ptr} + 16]",   // get b from chunk

        // multiply and add all pairs of pixel channels simultaneously
        "pmaddubsw      {a},                {multipliers}",
        "pmaddubsw      {b},                {multipliers}",

        // horizontally add the channel pairs into final sums
        "phaddw         {a},                {b}",

        // cheating % 64
        "pand           {a},                {mod_64_mask}",     // a is now the hashes of the pixels originally in a and b

        "packuswb       {a},                {a}",               // a becomes the final 8 hashes in byte form, duplicated
        "movq           [{hashes_ptr}],     {a}",               // put a into list of hash results

        in_ptr          = in(reg)           bytes.as_ptr(),
        hashes_ptr      = in(reg)           output.as_ptr(),

        multipliers_ptr = in(reg)           &HASHING_NUMS_RGBA,
        mask_ptr        = in(reg)           &MOD64MASK,
        multipliers     = out(xmm_reg)      _,
        mod_64_mask     = out(xmm_reg)      _,

        a               = out(xmm_reg)      _,
        b               = out(xmm_reg)      _,

        options(preserves_flags, nostack)
    );

    output.set_len(count);
    return output;
}

/// A variation on zakarumych's hashing function from rapid-qoi, but with one less & instruction
fn hash_rgba(pixel: [u8; 4]) -> u64 {
    let pixel = u32::from_ne_bytes(pixel) as u64;

    // the first two lines do the same as rqpid-qoi
    let duplicated = pixel * 0x0000000100000001_u64;
    let a0g00b0r = duplicated & 0xff00ff0000ff00ff_u64;
    // this magic number puts the hash in the top 6 bits instead of the top 8
    let hash_high6 = a0g00b0r * 0x0c001c000014002c_u64;
    let hash = hash_high6 >> 58; // now there's no need for the last mask

    return hash;
}
