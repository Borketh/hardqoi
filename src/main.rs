extern crate alloc;
extern crate bytemuck;

use bytemuck::cast_slice;
use std::ops::Div;
use std::path::Path;
use std::time::{Duration, Instant};

use image::{io::Reader, DynamicImage, ImageFormat};

use lib::common::QOIHeader;
use lib::{decode, encode};

use crate::common::RGBA;
pub use lib::*;

// keep this here so imports work when building with this as root
#[path = "./lib.rs"]
mod lib;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let filename = &args[1];
    let (format, image) = open_file(filename);
    let new_filename = {
        let format_ext = format!("{format:?}").to_ascii_lowercase();
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

    let raw = cast_slice::<u8, RGBA>(img.as_bytes()).to_vec();

    let mut qoi_data = Vec::with_capacity(raw.len() / 8);
    match encode(&raw, &mut qoi_data, meta) {
        Ok(_) => write_qoi(&qoi_data, filename).unwrap(),
        Err((found, expected)) => panic!(
            "Expected {} pixels, found {} pixels instead",
            expected, found
        ),
    };
    let mut decoded: Vec<RGBA> = Vec::with_capacity(raw.len());
    match decode(&qoi_data, &mut decoded) {
        Ok(()) => {
            let decodedpx = decoded.as_slice();
            assert_eq!(
                raw.len(),
                decodedpx.len(),
                "Input and output sizes do not match!"
            );
            for i in 0..decoded.len() {
                assert_eq!(raw[i], decodedpx[i], "There is a discrepancy between the input and the decoded output at position {}: Expected: 0x{:08x}, Got: 0x{:08x}", i, raw[i], decodedpx[i]);
            }
            println!("Successful trial run, Beginning benchmarking");

            let mut encode_time_sum: Duration = Duration::from_secs(0);
            let mut decode_time_sum: Duration = Duration::from_secs(0);
            let iterations = 1000;

            for _ in 0..iterations {
                qoi_data.clear();
                decoded.clear();
                let encode_time = Instant::now();
                encode(&raw, &mut qoi_data, meta).unwrap();
                encode_time_sum += encode_time.elapsed();
                let decode_time = Instant::now();
                decode(&qoi_data, &mut decoded).unwrap();
                decode_time_sum += decode_time.elapsed();
            }
            println!("Encode time: {:?}", encode_time_sum.div(iterations));
            println!("Decode time: {:?}", decode_time_sum.div(iterations));
        }
        Err((read, expected)) => panic!(
            "Expected {} pixels, found {} pixels instead",
            expected, read
        ),
    }
}

pub fn write_qoi(data: &[u8], filename: &str) -> Result<(), std::io::Error> {
    let mut f = std::fs::File::create(filename).expect("Unable to save QOI image!");
    use std::io::Write;
    f.write_all(data)
}
