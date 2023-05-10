use alloc::vec::Vec;
use core::arch::asm;

use crate::common::{HASH, RGBA};
pub(crate) use crate::Hashing;

use super::HASH_RGBA_MANY;

static MOD64MASK: u64 = 0x003f003f003f003fu64;
static HASHING_NUMS_RGBA: u64 = 0x0b0705030b070503u64;

impl Hashing for [RGBA; 64] {
    fn update(&mut self, pixel_feed: &[RGBA]) {
        match pixel_feed.len() {
            0 => (),
            1 => {
                self.swap(pixel_feed.first().unwrap());
            }
            _ => {
                pixel_feed
                    .iter()
                    .zip(hashes_rgba(pixel_feed).iter())
                    .for_each(|(&pixel, &hash)| *unsafe { self.fetch_mut(hash) } = pixel);
            }
        };
    }

    unsafe fn fetch(&self, hash: HASH) -> &RGBA {
        self.get_unchecked(hash as usize)
    }

    unsafe fn fetch_mut(&mut self, hash: HASH) -> &mut RGBA {
        self.get_unchecked_mut(hash as usize)
    }

    #[inline(always)]
    fn swap(&mut self, pixel: &RGBA) -> (RGBA, HASH) {
        let hash = hash_rgba(pixel);
        let pixel2 = core::mem::replace(unsafe { self.fetch_mut(hash) }, *pixel);
        (pixel2, hash)
    }
}
/// Unsigned quotient and remainder without checking for zero divisor
fn div_rem(n: usize, d: usize) -> (usize, usize) {
    let (q, r): (usize, usize);

    unsafe {
        #[cfg(target_pointer_width = "64")]
        asm!(
        "div {divisor:r}",
        divisor = in(reg) d,
        inout("rax") n => q,
        inout("rdx") 0usize => r,
        options(nostack, pure, nomem)
        );
        #[cfg(target_pointer_width = "32")]
        asm!(
        "div {divisor:e}",
        divisor = in(reg) d,
        inout("eax") n => q,
        inout("edx") 0usize => r,
        options(nostack, pure, nomem)
        );
    }

    (q, r)
}

pub fn hashes_rgba(pixels: &[RGBA]) -> Vec<HASH> {
    // this wraps the "unsafe" enclosed function to make the most efficient use of SIMD
    let count = pixels.len();
    #[cfg(target_feature = "ssse3")]
    if count <= 8 {
        return unsafe { simd_hashes_lt8(pixels, count) };
    }
    unsafe {
        let chunk_size = HASH_RGBA_MANY.hash_chunk_size();
        let (chunk_count, _) = div_rem(count, chunk_size);
        let full_chunk_space = chunk_count * chunk_size;
        let mut hashes: Vec<HASH> = Vec::with_capacity(full_chunk_space + chunk_size);
        // println!("Trying to write {} hashes into {} bytes", count, hashes.capacity());
        // println!("The hash buffer starts at {:?}", hashes.as_ptr());
        HASH_RGBA_MANY.hash_chunks(pixels.as_ptr(), hashes.as_mut_ptr(), chunk_count + 1);
        hashes.set_len(full_chunk_space); // don't remove this line you doorknob
        for pixel in &pixels[hashes.len()..] {
            hashes.push(hash_rgba(pixel))
        }
        hashes
    }
}

#[inline(always)] // because it's wrapped by the above function, a nested call isn't useful
#[cfg(target_feature = "ssse3")]
/// A stripped down SIMD hashing for pixel counts between 2 and 8 (inclusive)
unsafe fn simd_hashes_lt8(bytes: &[RGBA], count: usize) -> Vec<HASH> {
    let mut output: Vec<u8> = Vec::with_capacity(8);

    asm!(
        "movddup    {multipliers},  [{multipliers_ptr}]",
        "movddup    {round_mask},   [{mask_ptr}]",

        // load up to 8 pixels into two xmm registers
        "movdqu     {pixels_a},     [{in_ptr}]",        // get a from chunk
        "movdqu     {pixels_b},     [{in_ptr} + 16]",   // get b from chunk

        // multiply and add all pairs of pixel channels simultaneously
        "pmaddubsw  {pixels_a},     {multipliers}",
        "pmaddubsw  {pixels_b},     {multipliers}",

        // horizontally add the channel pairs into final sums
        "phaddw     {pixels_a},     {pixels_b}",

        // cheating % 64
        "pand       {pixels_a},     {round_mask}",

        "packuswb   {pixels_a},     {pixels_a}",    // a becomes the final 8 hashes in byte form, duplicated
        "movq       [{hashes_ptr}], {pixels_a}",    // put them into list of hash results

        in_ptr      = in(reg)       bytes.as_ptr(),
        hashes_ptr  = in(reg)       output.as_ptr(),

        multipliers_ptr = in(reg)   &HASHING_NUMS_RGBA,
        mask_ptr    = in(reg)       &MOD64MASK,
        multipliers = out(xmm_reg)  _,
        round_mask  = out(xmm_reg)  _,

        pixels_a    = out(xmm_reg)  _,
        pixels_b    = out(xmm_reg)  _,

        options(preserves_flags, nostack)
    );

    output.set_len(count);
    output
}

/// A variation on zakarumych's hashing function from rapid-qoi, but with one less & instruction
pub fn hash_rgba(pixel: &RGBA) -> HASH {
    let pixel = *pixel as u64;

    // the first two lines do the same as rapid-qoi
    let duplicated = pixel.wrapping_mul(0x0000000100000001_u64);
    let a0g00b0r = duplicated & 0xff00ff0000ff00ff_u64;
    // this magic number puts the hash in the top 6 bits instead of the top 8
    let hash_high6 = a0g00b0r.wrapping_mul(0x0c001c000014002c_u64);
    let hash = hash_high6 >> 58; // now there's no need for the last mask

    hash as HASH
}
