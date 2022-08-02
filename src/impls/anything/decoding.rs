use crate::common::{
    QOIHeader, END_8, QOI_OP_DIFF, QOI_OP_INDEX, QOI_OP_LUMA, QOI_OP_RGB, QOI_OP_RGBA, QOI_OP_RUN,
};
use crate::qoi::hashing;
use wrapping_rgba::{BIAS_2, PIXEL, ZERO_PIXEL};

#[path = "wrapping_rgba.rs"]
mod wrapping_rgba;

pub fn decode(input: &Vec<u8>, output: &mut Vec<[u8; 4]>) -> Result<(), (usize, usize)> {
    let len: usize = input.len() - 8;
    let mut prev_pixel: [u8; 4] = [0, 0, 0, 255];
    let mut hash_indexed_array: [[u8; 4]; 64] = [ZERO_PIXEL.rgba_arr(); 64];

    let header = QOIHeader::from(<[u8; 14]>::from(input[0..14].try_into().unwrap()));
    let mut pos: usize = 14;

    while pos < len {
        let next_op: u8 = input[pos];
        match next_op {
            QOI_OP_RGBA => {
                pos += 1;
                prev_pixel[0] = input[pos];
                pos += 1;
                prev_pixel[1] = input[pos];
                pos += 1;
                prev_pixel[2] = input[pos];
                pos += 1;
                prev_pixel[3] = input[pos];
                output.push(prev_pixel);
            }
            QOI_OP_RGB => {
                pos += 1;
                prev_pixel[0] = input[pos];
                pos += 1;
                prev_pixel[1] = input[pos];
                pos += 1;
                prev_pixel[2] = input[pos];
                // alpha remains unchanged
                output.push(prev_pixel);
            }
            _ => match next_op & 0b11000000 {
                QOI_OP_DIFF => {
                    let diff = PIXEL::from(op_diff_expand222(next_op));
                    prev_pixel = (PIXEL::from(prev_pixel) + diff - BIAS_2).rgba_arr();
                    output.push(prev_pixel);
                }
                QOI_OP_LUMA => {
                    pos += 1;
                    let diff = PIXEL::from(op_luma_expand644(next_op, input[pos]));
                    prev_pixel = (PIXEL::from(prev_pixel) + diff).rgba_arr();
                    output.push(prev_pixel);
                }
                QOI_OP_RUN => {
                    let run_count = (next_op & 0b00111111) + 1;
                    output.extend(core::iter::repeat(prev_pixel).take(run_count as usize));
                }
                QOI_OP_INDEX => {
                    let index = (next_op & 0b00111111) as usize;
                    prev_pixel = hash_indexed_array[index];
                    output.push(prev_pixel);
                    pos += 1;
                    continue; // no need to hash if it's there already
                }
                _ => panic!("YOUR CPU'S AND GATE IS BROKEN"),
            }, // end match 2-bit
        } // end match 8-bit
        hash_indexed_array[hashing::hash_rgba(prev_pixel) as usize] = prev_pixel;
        pos += 1;
    } // end loop

    assert_eq!(
        input[(pos)..(pos + 8)],
        END_8,
        "QOI file does not end normally! Found {:?} instead",
        &input[(pos)..(pos + 8)]
    );

    if header.image_size() == output.len() {
        Ok(())
    } else {
        Err((output.len(), header.image_size()))
    }
}

/// An easily inlinable function that expands the OP_DIFF byte into an array
/// 0b_01_dr_dg_db_u8 -> [000000dr_u8, 000000dg_u8, 000000db_u8, 0u8]
fn op_diff_expand222(x: u8) -> [u8; 4] {
    // thanks for this function, https://github.com/adrianparvino ! :)
    let y = (x as u32) * ((1 << 24) | (1 << 14) | (1 << 4));
    ((y & 0x03030303) >> 8).to_le_bytes()
}

fn op_luma_expand644(op_and_dg: u8, byte_2: u8) -> [u8; 4] {
    let dg_m8 = op_and_dg.wrapping_sub(0b10000000_u8 + 40u8);

    return [
        (byte_2 >> 4).wrapping_add(dg_m8),
        dg_m8.wrapping_add(8u8),
        (byte_2 & 0xf).wrapping_add(dg_m8),
        0,
    ];
}
