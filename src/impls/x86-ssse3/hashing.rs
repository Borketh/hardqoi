static MOD64MASK: u64 = 0x003f003f003f003fu64;
static HASHING_NUMS_RGBA: u64 = 0x0b0705030b070503u64;

#[cfg(target_feature = "ssse3")]
pub fn hashes_rgba(bytes: &Vec<u8>, count: usize) -> Vec<u8> {
    // this wraps the "unsafe" enclosed function to make the function pointer type
    // equivalent to other implementations of hashes_rgba
    return unsafe { hashes_rgba_ssse3_impl(bytes, count) };
}

#[inline] // because it's wrapped by the above function, a nested call isn't useful
#[cfg(target_feature = "ssse3")]
unsafe fn hashes_rgba_ssse3_impl(bytes: &Vec<u8>, count: usize) -> Vec<u8> {
    use std::arch::asm;

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
        "movddup xmm0, [{hash_multipliers}]",
        "movddup xmm1, [{mod_64_mask}]",
        hash_multipliers = in(reg) &HASHING_NUMS_RGBA,
        mod_64_mask = in(reg) &MOD64MASK,

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

            "packuswb {a}, {b}",    // a becomes the final 16 hashes in byte form
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
    asm!("sfence"); // to tell other cores where all that movntdq'd stuff came from
    hashes.set_len(count);

    return hashes;
}
