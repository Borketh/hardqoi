use bytemuck::checked::cast_slice;

pub fn hashes_rgba(bytes: &Vec<u8>, count: usize) -> Vec<u8> {
    let mut hashes: Vec<u8> = Vec::with_capacity(count);

    let pixels: &[[u8; 4]] = cast_slice::<u8, [u8; 4]>(bytes);

    for i in 0..count {
        hashes.push(
            { (pixels[i][0] * 3) + (pixels[i][1] * 5) + (pixels[i][2] * 7) + (pixels[i][3] * 11) }
                & 0b00111111u8,
        );
    }
    return hashes;
}
