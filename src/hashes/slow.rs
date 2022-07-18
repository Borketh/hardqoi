pub fn hashes_rgba(bytes: &Vec<u8>, count: usize) -> Vec<u8> {
    let safe_iterations = count / 16 + 1;
    let safe_alloc_bytes = safe_iterations * 16;
    // Allocate 1-16 bytes extra for the hashes vector, so that writing the final __m128i doesn't
    // overwrite anything that comes after it and corrupt anything. The capacity should not change,
    // but the size should be set after writing everything.
    let mut hashes: Vec<u8> = Vec::with_capacity(safe_alloc_bytes);

    let pixels: &Vec<[u8; 4]> = unsafe { &*(bytes as *const Vec<u8> as *const Vec<[u8; 4]>) };

    for i in 0..count {

        let slice: [u8; 4] = pixels[i];

        hashes.push(((slice[0] * 3) + (slice[1] * 5) + (slice[2] * 7) + (slice[3] * 11)) & 0b00111111u8);
    }
    return hashes;
}


