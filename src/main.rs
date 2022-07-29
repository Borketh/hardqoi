extern crate bytemuck;

use std::path::Path;

use image::{io::Reader, DynamicImage, ImageFormat};

use crate::common::QOIHeader;
use crate::qoi::{encoding::encode, write_qoi};
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
    let meta = QOIHeader::new(&img);
    img = DynamicImage::ImageRgba8(img.to_rgba8());
    let raw = img.as_bytes().to_vec();

    let mut qoi_data = Vec::with_capacity(raw.len() / 8);
    match encode(&raw, meta, &mut qoi_data) {
        Ok(_) => write_qoi(&qoi_data, filename).unwrap(),
        Err((found, expected)) => panic!(
            "Expected {} pixels, found {} pixels instead",
            expected, found
        ),
    }
}
