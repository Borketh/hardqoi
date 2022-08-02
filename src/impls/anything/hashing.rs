use bytemuck::checked::cast_slice;

pub fn hashes_rgba(bytes: &Vec<u8>, count: usize) -> Vec<u8> {
    let mut hashes: Vec<u8> = Vec::with_capacity(count);

    let pixels: &[[u8; 4]] = cast_slice::<u8, [u8; 4]>(bytes);

    for i in 0..count {
        hashes.push(hash_rgba(pixels[i]));
    }
    return hashes;
}

pub fn hash_rgba(pixel: [u8; 4]) -> u8 {
    ((pixel[0] * 3) + (pixel[1] * 5) + (pixel[2] * 7) + (pixel[3] * 11)) & 0b00111111u8
}
