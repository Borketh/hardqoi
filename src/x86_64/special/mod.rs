use super::hashing::hash_rgba;
use crate::common::RGBA;
use alloc::boxed::Box;
use lazy_static::lazy_static;

#[cfg(target_feature = "ssse3")]
mod v2;
#[cfg(target_feature = "avx")]
mod v3;
#[cfg(target_feature = "avxvnni")]
mod v3n;
#[cfg(target_feature = "avx512bw")]
mod v4;
#[cfg(target_feature = "avx512vnni")]
mod v4n;

lazy_static! {
    pub(crate) static ref HASH_RGBA_MANY: Box<dyn VectorizedHashing> = get_hashing_function();
}

pub(crate) trait VectorizedHashing: Sync + Send {
    unsafe fn hash_chunks(
        &self,
        pixel_ptr: *const RGBA,
        hash_ptr: *mut u8,
        count: usize,
    ) -> (*const u32, *mut u8);
    fn hash_chunk_size(&self) -> usize;
}

#[cfg(target_feature = "avxvnni")]
mod avxvnnistub {
    pub(crate) struct ExtendedFeaturesLeaf1;

    impl ExtendedFeaturesLeaf1 {
        pub(crate) fn has_avxvnni(&self) -> bool {
            unimplemented!()
        }
    }

    pub(crate) fn avx_vnni_stub() -> Option<ExtendedFeaturesLeaf1> {
        unimplemented!()
    }
}

#[cfg(not(target_feature = "ssse3"))]
#[inline(always)]
pub(crate) fn get_hashing_function() -> Box<dyn VectorizedHashing> {
    Box::new(V1)
}

#[cfg(target_feature = "ssse3")]
pub(crate) fn get_hashing_function() -> Box<dyn VectorizedHashing> {
    use raw_cpuid::CpuId;
    let cpuid = CpuId::new();

    #[cfg(target_feature = "avx")]
    if let Some(extended_features) = cpuid.get_extended_feature_info() {
        #[cfg(target_feature = "avx512vnni")]
        if extended_features.has_avx512vnni() {
            // First choice
            use v4n::AVX512VNNI;
            return Box::new(AVX512VNNI);
        }

        #[cfg(target_feature = "avx512bw")]
        if extended_features.has_avx512bw() {
            // Second choice
            use v4::AVX512;
            return Box::new(AVX512);
        }

        #[cfg(target_feature = "avxvnni")]
        if let Some(ext_feats2) = avxvnnistub::avx_vnni_stub() {
            if ext_feats2.has_avxvnni() {
                // Third choice
                use v3n::AVXVNNI;
                return Box::new(AVXVNNI);
            }
        }
        #[cfg(target_feature = "avx")]
        if extended_features.has_avx2() {
            // Fourth choice
            use v3::AVX;
            return Box::new(AVX);
        }
    }
    // if the extended features struct can't be obtained or none of the mathods from above returned,
    // try to see if the processor at least supports ssse3
    #[cfg(target_feature = "ssse3")]
    if let Some(features) = cpuid.get_feature_info() {
        if features.has_ssse3() {
            // Fifth choice
            use v2::SSSE3;
            return Box::new(SSSE3);
        }
    }
    // If nothing else returns early, return the least optimized function
    return Box::new(V1);
}

// pub(crate) fn ssse3_hash_rgba(rgba_bytes: &Vec<u32>, pixel_count: usize) -> Vec<u8> {
//     let (chunks, remainder) = div_rem(pixel_count as u64, chunk_size);
//     let mut hashes: Vec<u8> = Vec::with_capacity(pixel_count);
//     let mut pixel_pointer = rgba_bytes.as_ptr();
//     let mut hash_pointer = hashes.as_mut_ptr();
//     let mut remainder = pixel_count - (chunks_of_48 * 48);
//     unsafe {
//         (pixel_pointer, hash_pointer) =
//             hash_chunks_of_48(pixel_pointer, hash_pointer, chunks_of_48);
//         while remainder > 16 {
//             (pixel_pointer, hash_pointer) = hash_chunk_of_16(pixel_pointer, hash_pointer);
//             remainder -= 16;
//         }
//         while remainder > 0 {
//             *hash_pointer = hash_single_rgba(pixel_pointer.as_ref().unwrap());
//             pixel_pointer = pixel_pointer.add(1);
//             hash_pointer = hash_pointer.add(1);
//             remainder -= 1;
//         }
//         return hashes;
//     }
// }

pub struct V1;

impl VectorizedHashing for V1 {
    unsafe fn hash_chunks(
        &self,
        mut pixel_ptr: *const u32,
        mut hash_ptr: *mut u8,
        count: usize,
    ) -> (*const u32, *mut u8) {
        for _ in 0..count {
            *hash_ptr = hash_rgba(pixel_ptr.as_ref().unwrap());
            pixel_ptr = pixel_ptr.add(1);
            hash_ptr = hash_ptr.add(1);
        }
        (pixel_ptr, hash_ptr)
    }

    fn hash_chunk_size(&self) -> usize {
        1
    }
}
