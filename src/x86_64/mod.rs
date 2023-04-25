extern crate lazy_static;
extern crate raw_cpuid;

pub(crate) mod decode;
pub(crate) mod encode;
pub(crate) mod hashing;
pub(crate) mod special;

pub(crate) use special::HASH_RGBA_MANY;
