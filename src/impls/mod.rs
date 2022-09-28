// The cargo-asm tool has spoiled my perceptions of how smart the compiler really is,
// hence my micromanaging exactly which instructions are called at critical parts of the code.
// My only regret is that it took longer, but the benefits are really good!
// I intend to make other platform-specific implementations once the base and x86 are finished.
#[cfg_attr(SVE2, path = "aarch64/sve2/mod.rs")]
#[cfg_attr(SVE, path = "aarch64/sve/mod.rs")]
#[cfg_attr(NEON, path = "aarch64/neon/mod.rs")]
#[cfg_attr(AVX_512_VNNI, path = "x86_64/avx-512-vnni/mod.rs")]
#[cfg_attr(AVX_512, path = "x86_64/avx-512/mod.rs")]
#[cfg_attr(AVX_VNNI, path = "x86_64/avx-vnni/mod.rs")]
#[cfg_attr(AVX, path = "x86_64/avx/mod.rs")]
#[cfg_attr(AVX_VNNI_HALF, path = "x86_64/avx-vnni-half/mod.rs")]
#[cfg_attr(SSSE3, path = "x86_64/ssse3/mod.rs")]
#[cfg_attr(NONE, path = "anything/mod.rs")]
pub mod implementation;
