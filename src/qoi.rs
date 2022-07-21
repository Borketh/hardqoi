#[cfg_attr(
    all(feature = "use_simd", target_feature = "ssse3"),
    path = "impl/x86-ssse3/lib.rs"
)]
#[cfg_attr(
    any(not(feature = "use_simd"), not(target_feature = "ssse3")),
    path = "impl/anything/lib.rs"
)]
mod qoi;
pub use qoi::*;