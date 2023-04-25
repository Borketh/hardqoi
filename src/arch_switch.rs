// The cargo-asm tool has spoiled my perceptions of how smart the compiler really is,
// hence my micromanaging exactly which instructions are called at critical parts of the code.
// My only regret is that it took longer, but the benefits are really good!
// I intend to make other platform-specific implementations once the base and x86 are finished.
#[cfg_attr(target_arch = "x86_64", path = "x86_64/mod.rs")]
#[cfg_attr(target_arch = "aarch64", path = "aarch64/mod.rs")]
pub mod implementation;
