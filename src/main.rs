extern crate bytemuck;

use std::path::Path;

use image::{io::Reader, DynamicImage, ImageFormat};

use crate::common::QOIHeader;
use crate::qoi::{decoding::decode, encoding::encode, write_qoi};
pub use qoi::common;

pub mod qoi;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    println!("{:?}", args);

    let filename = &args[1];
    let (format, image) = open_file(filename);
    let new_filename = {
        let format_ext = format!("{format:?}").to_ascii_lowercase();
        dbg!(&format_ext);
        filename.replace(format_ext.as_str(), "qoi")
    };
    img_to_qoi(image, new_filename.as_str());
}

pub fn open_file(path: &str) -> (ImageFormat, DynamicImage) {
    let path = Path::new(path);
    let reader = match Reader::open(path) {
        Ok(reader) => reader,
        Err(why) => panic!("Failed to open {}: {}", path.display(), why),
    };

    (
        reader.format().unwrap(),
        reader.decode().expect("Decoding error"),
    )
}

fn img_to_qoi(mut img: DynamicImage, filename: &str) {
    let meta = QOIHeader::from(&img);
    img = DynamicImage::ImageRgba8(img.to_rgba8());
    let raw = img.as_bytes().to_vec();

    let mut qoi_data = Vec::with_capacity(raw.len() / 8);
    match encode(&raw, meta, &mut qoi_data) {
        Ok(_) => write_qoi(&qoi_data, filename).unwrap(),
        Err((found, expected)) => panic!(
            "Expected {} pixels, found {} pixels instead",
            expected, found
        ),
    };
    let mut decoded: Vec<[u8; 4]> = Vec::with_capacity(raw.len() / 4);
    match decode(&qoi_data, &mut decoded) {
        Ok(()) => {
            let rawpx = bytemuck::cast_slice::<u8, [u8; 4]>(&raw);
            let decodedpx = decoded.as_slice();
            assert_eq!(rawpx.len(), decodedpx.len(), "Input and output sizes do not match!");
            for i in 0..decoded.len() {
                assert_eq!(rawpx[i], decodedpx[i], "There is a discrepancy between the input and the decoded output at position {}: Expected: {:?}, Got: {:?}", i, rawpx[i], decodedpx[i]);
            }
        }
        Err((read, expected)) => panic!(
            "Expected {} pixels, found {} pixels instead",
            expected, read
        ),
    }
}
