use super::hashing::{HashIndexedArray, Hashing};
use crate::alloc::vec::Vec;
use crate::common::{
    QOIHeader, END_8, QOI_OP_DIFF, QOI_OP_INDEX, QOI_OP_LUMA, QOI_OP_RGB, QOI_OP_RGBA, QOI_OP_RUN,
};
use wrap_math_pixel::{BIAS_2, PIXEL};

#[path = "wrap_math_pixel.rs"]
mod wrap_math_pixel;

pub fn decode(input: &Vec<u8>, output: &mut Vec<[u8; 4]>) -> Result<(), (usize, usize)> {
    let len: usize = input.len() - 8;
    let mut prev_pixel: [u8; 4] = [0, 0, 0, 255];
    let mut hash_indexed_array = HashIndexedArray::new();
    let mut last_hash_update = 0;

    let header = QOIHeader::from(<[u8; 14]>::from(input[0..14].try_into().unwrap()));
    let mut pos: usize = 14;

    // if the first op is a run, black ends up not in the HIA because of the hash-skipping behaviour
    if (QOI_OP_RUN..QOI_OP_RGB).contains(&input[pos]) {
        // this fixes that
        hash_indexed_array.push(prev_pixel);
    }

    while pos < len {
        let next_op: u8 = input[pos];
        match next_op {
            QOI_OP_RGBA => {
                pos += 1;
                let end = pos + 3;
                prev_pixel = input[pos..=end].try_into().unwrap();
                pos = end;
            }
            QOI_OP_RGB => {
                pos += 1;
                prev_pixel = [input[pos], input[pos + 1], input[pos + 2], prev_pixel[3]];
                pos += 2;
                // alpha remains unchanged
            }
            _ => match next_op & 0b11000000 {
                QOI_OP_DIFF => {
                    let diff = PIXEL::from(op_diff_expand222(next_op));
                    prev_pixel = (PIXEL::from(prev_pixel) + diff - BIAS_2).rgba_arr();
                }
                QOI_OP_LUMA => {
                    pos += 1;
                    let diff = PIXEL::from(op_luma_expand644(next_op, input[pos]));
                    prev_pixel = (PIXEL::from(prev_pixel) + diff).rgba_arr();
                }
                QOI_OP_RUN => {
                    hash_indexed_array.update(&output[last_hash_update..]);
                    let mut run_count = (next_op as usize & 0x3f) + 1;
                    loop {
                        pos += 1;
                        if (QOI_OP_RUN..QOI_OP_RGB).contains(&input[pos]) {
                            let additional = (input[pos] & 0x3f) as usize + 1;
                            run_count += additional;
                        } else {
                            break;
                        }
                    }

                    output.extend(core::iter::repeat(prev_pixel).take(run_count as usize));
                    last_hash_update = output.len(); // no need to repeat hashing updates on the same pixel
                    continue;
                }
                QOI_OP_INDEX => {
                    hash_indexed_array.update(&output[last_hash_update..]);
                    last_hash_update = output.len();

                    let index = next_op & 0b00111111;
                    prev_pixel = hash_indexed_array.fetch(index);
                }
                _ => panic!("YOUR CPU'S AND GATE IS BROKEN"),
            }, // end match 2-bit
        } // end match 8-bit
        output.push(prev_pixel);
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
