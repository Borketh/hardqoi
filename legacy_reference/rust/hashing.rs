use crate::alloc::vec::Vec;
pub use crate::{HashIndexedArray, Hashing};

impl Hashing for HashIndexedArray {
    fn update(&mut self, pixel_feed: &[[u8; 4]]) {
        let hashes = hashes_rgba(pixel_feed);
        for i in 0..hashes.len() {
            self.indices_array[hashes[i] as usize] = pixel_feed[i];
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

pub fn hashes_rgba(pixels: &[[u8; 4]]) -> Vec<u8> {
    let mut hashes: Vec<u8> = Vec::with_capacity(pixels.len());

    for i in 0..pixels.len() {
        hashes.push(hash_rgba(pixels[i]));
    }
    return hashes;
}

fn hash_rgba(pixel: [u8; 4]) -> u8 {
    ((pixel[0] * 3) + (pixel[1] * 5) + (pixel[2] * 7) + (pixel[3] * 11)) & 0b00111111u8
}
