mod qoi;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    println!("{:?}", args);

    let filename = &args[1];
    let image = open_file(filename);
    img_to_qoi(image)
}

use std::ops::{Add, Div};
use std::path::Path;
use std::time::{Duration, Instant};

use image::io::Reader;
use image::{ColorType, DynamicImage, GenericImageView};

// The cargo-asm tool has spoiled my perceptions of how smart the compiler really is,
// hence my micromanaging exactly which instructions are called at critical parts of the code.
// My only regret is that it took longer, but the benefits are really good!
// I intend to make other platform-specific implementations once the base and x86 are finished.

use qoi::encoding::encode_raw_to_qoi;
use qoi::hashing::hashes_rgba;

pub fn open_file(path: &str) -> DynamicImage {
    let path = Path::new(path);
    let reader = match Reader::open(path) {
        Ok(reader) => reader,
        Err(why) => panic!("Failed to open {}: {}", path.display(), why),
    };

    reader.decode().expect("Decoding error")
}

pub fn img_to_qoi(mut img: DynamicImage) {
    let n_pixels = {
        let dims = img.dimensions();
        dims.0 as usize * dims.1 as usize
    };

    let rgba_conv_timer = Instant::now();
    let colour_type = img.color();
    match colour_type {
        ColorType::Rgb8 | ColorType::Rgba8 => img = DynamicImage::ImageRgba8(img.into_rgba8()),
        _ => panic!("The {colour_type:?} format is not supported by the QOI format (yet)"),
    };
    let rgba_conv_dur = rgba_conv_timer.elapsed();
    println!("{rgba_conv_dur:?} to convert {colour_type:?} to RGBA8");
    let raw_pixel_data = img.as_bytes().to_vec();

    println!("This is an RGB/RGBA image!");
    let timekeeper = Instant::now();
    let hashes = hashes_rgba(&raw_pixel_data, n_pixels);
    let duration = timekeeper.elapsed();

    println!("{:?}: {} hashes", duration, hashes.len());
    println!(
        "R {} G {} B {} A {} -> {}",
        raw_pixel_data.get(0).expect("Red not found"),
        raw_pixel_data.get(1).expect("Green not found"),
        raw_pixel_data.get(2).expect("Blue not found"),
        raw_pixel_data.get(3).expect("Alpha not found"),
        hashes.get(0).expect("Hash not found")
    );
    println!(
        "R {} G {} B {} A {} -> {}",
        raw_pixel_data.get(4).expect("Red not found"),
        raw_pixel_data.get(5).expect("Green not found"),
        raw_pixel_data.get(6).expect("Blue not found"),
        raw_pixel_data.get(7).expect("Alpha not found"),
        hashes.get(1).expect("Hash not found")
    );
    // assert_eq!(*hashes.get(0).unwrap(), 54u8);

    let iterations = 1024u32;

    let mut times: Vec<Duration> = Vec::with_capacity(iterations as usize);

    for iteration in 0..iterations {
        let timekeeper = Instant::now();
        let hashes = hashes_rgba(&raw_pixel_data, n_pixels);
        let duration = timekeeper.elapsed();
        times.push(duration);
        let size_mebibytes = hashes.len() as f64 / 1_048_576.0f64;
        let rate = size_mebibytes / (duration.as_nanos() as f64 / 1000000000.0f64);
        println!("Iteration {iteration}: {duration:?} | {rate:.4} MiB/s");
    }
    let mut total_duration: Duration = Duration::from_secs(0);
    for duration in times {
        total_duration = total_duration.add(duration);
    }
    let avg_dur = total_duration.div(iterations);
    let size_mebibytes = hashes.len() as f64 / 1_048_576.0f64;
    let avg_rate = size_mebibytes / (avg_dur.as_nanos() as f64 / 1000000000.0f64);
    println!("Average time: {avg_dur:?}   | Average rate: {avg_rate:.4} MiB/s");
}
