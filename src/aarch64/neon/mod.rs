// THIS IS A STUB AND WILL NOT RUN

pub mod decoding {
    use crate::alloc::vec::Vec;

    pub fn decode(_input: &Vec<u8>, _output: &mut Vec<[u8; 4]>) -> Result<(), (usize, usize)> {
        todo!();
    }
}
pub mod encoding {
    use crate::alloc::vec::Vec;
    use crate::common::QOIHeader;

    pub fn encode(
        _raw: &Vec<u8>,
        _meta: QOIHeader,
        _buf: &mut Vec<u8>,
    ) -> Result<usize, (usize, usize)> {
        todo!();
    }
}
pub mod hashing {
    use crate::alloc::vec::Vec;
    pub use crate::{HashIndexedArray, Hashing};

    pub fn hashes_rgba(_bytes: &Vec<u8>, _count: usize) -> Vec<u8> {
        todo!();
    }

    impl Hashing for HashIndexedArray {
        fn update(&mut self, _pixel_feed: &[[u8; 4]]) {
            todo!();
        }

        fn fetch(&mut self, _hash: u8) -> [u8; 4] {
            todo!();
        }

        fn push(&mut self, _pixel: [u8; 4]) -> ([u8; 4], u8) {
            todo!();
        }

        fn new() -> Self {
            todo!();
        }
    }
}
