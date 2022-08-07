// The cargo-asm tool has spoiled my perceptions of how smart the compiler really is,
// hence my micromanaging exactly which instructions are called at critical parts of the code.
// My only regret is that it took longer, but the benefits are really good!
// I intend to make other platform-specific implementations once the base and x86 are finished.
#[cfg_attr(
    all(feature = "use_simd", target_feature = "ssse3"),
    path = "x86-ssse3/mod.rs"
)]
#[cfg_attr(
    any(not(feature = "use_simd"), not(target_feature = "ssse3")),
    path = "anything/mod.rs"
)]
pub mod implementation;
