static MOD_64: u128 = 0x003f003f003f003f003f003f003f003fu128;
static HASH_NUMS: u128 = 0x0b0705030b0705030b0705030b070503u128;

#[cfg(target_feature = "ssse3")]
pub fn hashes_rgba(bytes: &Vec<u8>, count: usize) -> Vec<u8> {
    // this wraps the "unsafe" enclosed function to make the function pointer type equivalent
    // to other implementations of HASHES_RGBA
    unsafe { hashes_rgba_ssse3_impl(bytes, count) }
}

#[inline]
#[cfg(target_feature = "ssse3")]
unsafe fn hashes_rgba_ssse3_impl(bytes: &Vec<u8>, count: usize) -> Vec<u8> {
    use std::arch::asm;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::__m128i;
    #[cfg(target_arch = "x86")]
    use std::arch::x86::__m128i;

    let safe_iterations = count / 16 + 1;
    let safe_alloc_bytes = safe_iterations * 16;
    // Allocate 1-16 bytes extra for the hashes vector, so that writing the final __m128i doesn't
    // overwrite anything that comes after it and corrupt anything. The capacity should not change,
    // but the size should be set after writing everything.
    let mut hashes: Vec<u8> = Vec::with_capacity(safe_alloc_bytes);

    let mut pixels_ptr: *const __m128i = bytes.as_ptr() as *const __m128i;
    let mut hashes_ptr = hashes.as_mut_ptr() as *mut __m128i;
    let mod_64_ptr: *const u128 = &MOD_64;
    let hash_nums_ptr: *const u128 = &HASH_NUMS;

    // reserve xmm0 and xmm1 for quick access of the hashing numbers and mod mask

    asm!(
    "movdqu xmm0, xmmword ptr [{hash_nums_ptr}]",
    "movdqu xmm1, xmmword ptr [{mod_64_ptr}]",
    hash_nums_ptr = in(reg) hash_nums_ptr,
    mod_64_ptr = in(reg) mod_64_ptr,

    out("xmm0") _,
    out("xmm1") _,
    options(readonly, preserves_flags)
    );

    for _ in 0..safe_iterations {
        // cargo-asm has spoiled my perceptions of how smart the compiler really is
        // hence my nannying exactly which instructions are called

        asm!(
        // load 16 pixels into four xmm registers
        "movdqa {a}, xmmword ptr [{pixels_ptr}]",         // get a from chunk
        "movdqa {b}, xmmword ptr [{pixels_ptr} + 16]",    // get b from chunk
        "movdqa {c}, xmmword ptr [{pixels_ptr} + 32]",    // get c from chunk
        "movdqa {d}, xmmword ptr [{pixels_ptr} + 48]",    // get d from chunk

        // hash a and b simultaneously
        "pmaddubsw {a}, xmm0",
        "pmaddubsw {b}, xmm0",
        "phaddw {a}, {b}",                                  // horizontal add
        "pand {a}, xmm1",                                   // % 64
        // a is now the i16x8 of the hashes of the pixels originally in a and b

        // hash c and d simultaneously
        "pmaddubsw {c}, xmm0",
        "pmaddubsw {d}, xmm0",
        "phaddw {c}, {d}",                                  // horizontal add
        "pand {c}, xmm1",                                   // % 64
        // c is now the i16x8 of the hashes of the pixels originally in  c and d

        // a becomes the final u8x16 of the 16 hashes
        "packuswb {a}, {b}",
        "movntdq xmmword ptr [{hashes_ptr}], {a}",          // put a into hashes

        pixels_ptr = in(reg) pixels_ptr,
        hashes_ptr = in(reg) hashes_ptr,

        // probably best to let these be set by the compiler
        a = out(xmm_reg) _,
        b = out(xmm_reg) _,
        c = out(xmm_reg) _,
        d = out(xmm_reg) _,
        out("xmm0") _, // reserved for hashing numbers
        out("xmm1") _, // reserved for mod 64 mask

        options(preserves_flags)
        );

        hashes_ptr = hashes_ptr.add(1);
        pixels_ptr = pixels_ptr.add(4);
    }

    hashes.set_len(count);

    return hashes;
}