extern crate bytemuck;
extern crate image;

use std::io::Write;
use std::ops::Div;
use std::path::Path;
use std::time::{Duration, Instant};

use bytemuck::cast_slice;
use image::{DynamicImage, ImageFormat, io::Reader};

use hardqoi::common::*;
use hardqoi::decode;
use hardqoi_avx512encode::encode;

fn open_file(path: &str) -> (ImageFormat, DynamicImage) {
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

fn write_file(data: &[u8], filename: &str) -> Result<(), std::io::Error> {
    let mut f = std::fs::File::create(filename).expect("Unable to save QOI image!");
    f.write_all(data)
}

fn test_and_bench_on_image(mut img: DynamicImage, filename: &str) {
    let meta = QOIHeader::from(&img);

    let png_decode_start = Instant::now();
    img = DynamicImage::ImageRgba8(img.to_rgba8());
    let raw = cast_slice::<u8, RGBA>(img.as_bytes()).to_vec();
    let png_decode_time = png_decode_start.elapsed();
    println!("Time to convert from PNG to raw RGBA u32s {png_decode_time:?}");

    let mut qoi_data = Vec::with_capacity(raw.len() / 8);
    match encode(&raw, &mut qoi_data, meta) {
        Ok(_) => write_file(&qoi_data, filename).unwrap(),
        Err((found, expected)) => panic!(
            "Expected {} pixels, found {} pixels instead",
            expected, found
        ),
    };
    let mut decoded: Vec<RGBA> = Vec::with_capacity(raw.len());
    match decode(&qoi_data, &mut decoded) {
        Ok(()) => {
            let decoded_pixels = decoded.as_slice();
            assert_eq!(
                raw.len(),
                decoded_pixels.len(),
                "Input and output sizes do not match!"
            );
            for i in 0..decoded.len() {
                assert_eq!(raw[i], decoded_pixels[i], "There is a discrepancy between the input and the decoded output at position {}: Expected: 0x{:08x}, Got: 0x{:08x}", i, raw[i], decoded_pixels[i]);
            }
            println!("Successful trial run, Beginning benchmarking");

            let mut encode_time_sum: Duration = Duration::from_secs(0);
            let mut decode_time_sum: Duration = Duration::from_secs(0);
            let iterations = 1;

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

fn test_and_bench(filename: &str) {
    let (format, image) = open_file(filename);
    let new_filename = {
        let format_ext = format!("{format:?}").to_ascii_lowercase();
        filename.replace(format_ext.as_str(), "qoi")
    };
    test_and_bench_on_image(image, new_filename.as_str());
}

fn compete_qoi_rs(png_path: &str) {
    use qoi_rs::{decode, encode, Image};
    let png_pixels = image::open(png_path).unwrap().into_rgba8();
    let png_img = Image {
        width: png_pixels.width() as u16,
        height: png_pixels.height() as u16,
        pixels: png_pixels.into_raw().into_boxed_slice(),
    };

    let encode_time_start = Instant::now();
    let our_qoi = encode(png_img, 4).unwrap();
    let encode_duration = encode_time_start.elapsed();
    println!("qoi_rs encode time: {encode_duration:?}");

    let decode_time_start = Instant::now();
    let img = decode(&our_qoi, 4).unwrap();
    let decode_duration = decode_time_start.elapsed();
    println!("qoi_rs decode time: {decode_duration:?}");

    // let qoi_path = png_path.replace("png", "qoi");
    // let reference_qoi = std::fs::read(qoi_path).unwrap().into_boxed_slice();

    // assert_eq!(our_qoi, reference_qoi);
}
fn compete_rapidqoi(png_path: &str) {
    use rapid_qoi::{Colors, Qoi};
    let png_pixels = image::open(png_path).unwrap().into_rgba8();
    let qoi_meta = Qoi {
        width: png_pixels.width(),
        height: png_pixels.height(),
        colors: Colors::Rgba,
    };
    let raw = png_pixels.into_raw();

    let encode_time_start = Instant::now();
    let our_qoi = qoi_meta.encode_alloc(raw.as_slice()).unwrap();
    let encode_duration = encode_time_start.elapsed();
    println!("rapidqoi encode time: {encode_duration:?}");

    let decode_time_start = Instant::now();
    Qoi::decode_alloc(&our_qoi).unwrap();
    let decode_duration = decode_time_start.elapsed();
    println!("rapidqoi decode time: {decode_duration:?}");

    // let qoi_path = png_path.replace("png", "qoi");
    // let reference_qoi = std::fs::read(qoi_path).unwrap().into_boxed_slice();

    // assert_eq!(our_qoi, reference_qoi);
}
#[test]
fn test_wonke() {
    test_and_bench("../../../test/wonke.png");
}

#[test]
fn test_thonk() {
    test_and_bench("../../../test/thonk.png");
}

#[test]
fn test_jw() {
    test_and_bench("../../../test/stephansquintet-jameswebb-giant.png");
}

#[test]
fn compete_jw() {
    compete_qoi_rs("../../../test/stephansquintet-jameswebb-giant.png");
    compete_rapidqoi("../../../test/stephansquintet-jameswebb-giant.png");
}

#[test]
fn test_jw_smol() {
    test_and_bench("../../../test/stephansquintet-jameswebb-clip.png");
}

#[test]
fn test_and_compete_jw() {
    test_jw();
    compete_jw();
}
