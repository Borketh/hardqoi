use image::{ColorType, DynamicImage, GenericImageView};
use std::ops::{Add, Div};

use image::io::Reader;
use std::path::Path;
use std::time::{Duration, Instant};

// cargo-asm has spoiled my perceptions of how smart the compiler really is
// hence my micromanaging exactly which instructions are called when I can
// I intend to make other platform-specific implementations
#[cfg_attr(target_feature = "ssse3", path = "hashes/ssse3.rs")]
#[cfg_attr(not(target_feature = "ssse3"), path = "hashes/slow.rs")]
mod hashes;
use hashes::hashes_rgba;

pub fn open_file(path: &str) -> DynamicImage {
    let path = Path::new(path);
    let reader = match Reader::open(path) {
        Ok(reader) => reader,
        Err(why) => panic!("Failed to open {}: {}", path.display(), why),
    };

    reader.decode().expect("Decoding error")
}

pub fn img_to_qoi(image: DynamicImage) {
    let colour_type = image.color();
    let length = {
        let dims = image.dimensions();
        dims.0 * dims.1
    } as usize;
    raw_to_qoi(image.into_rgba8().into_raw(), colour_type, length);
}

fn raw_to_qoi(bytes: Vec<u8>, colour_type: ColorType, count: usize) {
    match colour_type {
        ColorType::Rgb8 | ColorType::Rgba8 => {
            println!("This is an RGB/RGBA image!");
            let timekeeper = Instant::now();
            let hashes = hashes_rgba(&bytes, count);
            let duration = timekeeper.elapsed();

            println!("{:?}: {} hashes", duration, hashes.len());
            println!(
                "R {} G {} B {} A {} -> {}",
                bytes.get(0).expect("Red not found"),
                bytes.get(1).expect("Green not found"),
                bytes.get(2).expect("Blue not found"),
                bytes.get(3).expect("Alpha not found"),
                hashes.get(0).expect("Hash not found")
            );
            println!(
                "R {} G {} B {} A {} -> {}",
                bytes.get(4).expect("Red not found"),
                bytes.get(5).expect("Green not found"),
                bytes.get(6).expect("Blue not found"),
                bytes.get(7).expect("Alpha not found"),
                hashes.get(1).expect("Hash not found")
            );
            assert_eq!(*hashes.get(0).unwrap(), 54u8);

            let mut times: Vec<Duration> = Vec::with_capacity(32);

            for _ in 0..128 {
                let timekeeper = Instant::now();
                let hashes = hashes_rgba(&bytes, count);
                let duration = timekeeper.elapsed();
                times.push(duration);

                println!("{:?}: {} hashes", duration, hashes.len());
            }
            let mut total_duration: Duration = Duration::from_secs(0);
            for duration in times {
                total_duration = total_duration.add(duration);
            }
            println!("Average time: {:?}", total_duration.div(128));
        }
        _ => {
            panic!("The {colour_type:?} format is not supported by the QOI format (yet)")
        }
    }
}
