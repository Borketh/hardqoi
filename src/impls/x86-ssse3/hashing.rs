static MOD64MASK: u64 = 0x003f003f003f003fu64;
static HASHING_NUMS_RGBA: u64 = 0x0b0705030b070503u64;

pub use crate::{HashIndexedArray, Hashing};

impl Hashing for HashIndexedArray {
    fn update(&mut self, pixel_feed: &[[u8; 4]]) {
        let len = pixel_feed.len();
        if len == 0 {
            return;
        } else if len == 1 {
            self.indices_array[hash_rgba(pixel_feed[0]) as usize] = pixel_feed[0];
        } else {
            let bytes = bytemuck::cast_vec::<[u8; 4], u8>(Vec::from(pixel_feed));
            let hashes = hashes_rgba(&bytes, len);
            for i in 0..hashes.len() {
                self.indices_array[hashes[i] as usize] = pixel_feed[i];
            }
        }

    }

    fn fetch(&mut self, hash: u8) -> [u8; 4] {
        self.indices_array[hash as usize]
    }

    fn push(&mut self, pixel: [u8; 4]) -> ([u8; 4], u8) {
        let hash = hash_rgba(pixel);
        let pixel2 = core::mem::replace(&mut self.indices_array[hash as usize], pixel);
        (pixel2, hash)
    }

    fn new() -> Self {
        Self {
            indices_array: [[0, 0, 0, 0]; 64],
        }
    }
}

pub fn hashes_rgba(bytes: &Vec<u8>, count: usize) -> Vec<u8> {
    // this wraps the "unsafe" enclosed function to make the most efficient use of SIMD
    // dbg!(count);
    if count <= 8 {
        unsafe { simd_hashes_lt8(bytes, count) }
    } else {
        unsafe { simd_hashes_many(bytes, count) }
    }
}

#[inline(always)] // because it's wrapped by the above function, a nested call isn't useful
unsafe fn simd_hashes_many(bytes: &Vec<u8>, count: usize) -> Vec<u8> {
    use core::arch::asm;
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
        "movddup xmm0, [{half_hash_multipliers}]",
        "movddup xmm1, [{half_mod_64_mask}]",
        half_hash_multipliers = in(reg) &HASHING_NUMS_RGBA,
        half_mod_64_mask = in(reg) &MOD64MASK,

        out("xmm0") _,
        out("xmm1") _,
        options(readonly, preserves_flags, nostack)
    );

    for _ in 0..safe_iterations {
        asm!(
            // load 16 pixels into four xmm registers
            "movdqu {a}, xmmword ptr [{pixels_ptr}]",   // get b from chunk
            "add {pixels_ptr}, 16",
            "movdqu {b}, xmmword ptr [{pixels_ptr}]",   // get b from chunk
            "add {pixels_ptr}, 16",
            "movdqu {c}, xmmword ptr [{pixels_ptr}]",   // get c from chunk
            "add {pixels_ptr}, 16",
            "movdqu {d}, xmmword ptr [{pixels_ptr}]",   // get d from chunk
            "add {pixels_ptr}, 16",

            // multiply and add all pairs pixel channels simultaneously
            "pmaddubsw {a}, xmm0",
            "pmaddubsw {b}, xmm0",
            "pmaddubsw {c}, xmm0",
            "pmaddubsw {d}, xmm0",
            // horizontal add the channel pairs into final sums
            "phaddw {a}, {b}",
            "phaddw {c}, {d}",
            // cheating % 64
            "pand {a}, xmm1",       // a is now the hashes of the pixels originally in a and b
            "pand {c}, xmm1",       // c is now the hashes of the pixels originally in c and d

            "packuswb {a}, {c}",    // a becomes the final 16 hashes in byte form
            "movntdq xmmword ptr [{hashes_ptr}], {a}",  // put a into list of hash results
            "add {hashes_ptr}, 16",

            pixels_ptr = inout(reg) pixels_ptr,
            hashes_ptr = inout(reg) hashes_ptr,

            // probably best to let these be assigned by the assembler
            a = out(xmm_reg) _,
            b = out(xmm_reg) _,
            c = out(xmm_reg) _,
            d = out(xmm_reg) _,
            out("xmm0") _,          // reserved for hashing numbers
            out("xmm1") _,          // reserved for mod 64 mask

            options(preserves_flags, nostack)
        );
    }

    asm!("sfence"); // to tell other cores where all that movntdq'd stuff came from, which shouldn't affect anything
    hashes.set_len(count);

    return hashes;
}

/// A stripped down SIMD hashing for pixel counts between 2 and 8 (inclusive)
unsafe fn simd_hashes_lt8(bytes: &Vec<u8>, count: usize) -> Vec<u8> {
    let mut output: Vec<u8> = Vec::with_capacity(8);
    use core::arch::asm;

    asm!(
        "movddup {hash_multipliers}, [{half_hash_multipliers}]",
        "movddup {mod_64_mask}, [{half_mod_64_mask}]",

        // load 16 pixels into four xmm registers
        "movdqu {a}, xmmword ptr [{in_ptr}]",       // get a from chunk
        "movdqu {b}, xmmword ptr [{in_ptr} + 16]",  // get b from chunk

        // multiply and add all pairs pixel channels simultaneously
        "pmaddubsw {a}, xmm0",
        "pmaddubsw {b}, xmm0",

        // horizontal add the channel pairs into final sums
        "phaddw {a}, {b}",

        // cheating % 64
        "pand {a}, xmm1",                           // a is now the hashes of the pixels originally in a and b

        "packuswb {a}, {a}",                        // a becomes the final 16 hashes in byte form
        "movq [{hashes_ptr}], {a}",  // put a into list of hash results

        in_ptr = in(reg) bytes.as_ptr(),
        hashes_ptr = in(reg) output.as_ptr(),

        half_hash_multipliers = in(reg) &HASHING_NUMS_RGBA,
        half_mod_64_mask = in(reg) &MOD64MASK,
        hash_multipliers = out(xmm_reg) _,
        mod_64_mask = out(xmm_reg) _,

        // probably best to let these be assigned by the assembler
        a = out(xmm_reg) _,
        b = out(xmm_reg) _,

        options(preserves_flags, nostack)
    );

    output.set_len(count);
    return output;
}

fn hash_rgba(pixel: [u8; 4]) -> u8 {
    ((pixel[0] * 3) + (pixel[1] * 5) + (pixel[2] * 7) + (pixel[3] * 11)) & 0b00111111u8
}
