pub fn hashes_rgba(bytes: &Vec<u8>, count: usize) -> Vec<u8> {
    let safe_iterations = count / 16 + 1;
    let safe_alloc_bytes = safe_iterations * 16;
    // Allocate 1-16 bytes extra for the hashes vector, so that writing the final __m128i doesn't
    // overwrite anything that comes after it and corrupt anything. The capacity should not change,
    // but the size should be set after writing everything.
    let mut hashes: Vec<u8> = Vec::with_capacity(safe_alloc_bytes);

    let pixels = bytes.as_chunks::<4>().0;

    for i in 0..count {
        let [r, g, b, a] = pixels.get(i).unwrap();
        let hash = ((r * 3u8) + (g * 5u8) + (b * 7u8) + (a * 11u8)) % 64u8;
        hashes.push(hash);
    }
    return hashes;
}
