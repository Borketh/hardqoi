use std::intrinsics::likely;
use crate::ImageReader;
use image::{ColorType, DynamicImage, GenericImageView};

use std::path::Path;
use std::simd::*;
use std::time::Instant;

pub fn open_file(path: &str) -> DynamicImage {
    let path = Path::new(path);
    let reader = match ImageReader::open(path) {
        Ok(reader) => reader,
        Err(why) => panic!("Failed to open {}: {}", path.display(), why),
    };

    reader.decode().expect("Decoding error")
}

pub fn img_to_qoi(image: DynamicImage) {
    let colour_type = image.color();
    let length: usize = {
        let dims = image.dimensions();
        dims.0 * dims.1
    } as usize;
    raw_to_qoi(image.into_rgba8().into_raw(), colour_type, length);
}

fn raw_to_qoi(buffer: Vec<u8>, colour_type: ColorType, count: usize) {
    match colour_type {
        ColorType::Rgb8 => {
            println!("This is an 8-8-8 RGB image!");
            let timekeeper = Instant::now();
            let hashes = rgba_qoi(buffer, count);
            let duration = timekeeper.elapsed();
            println!("{:?}: {} hashes", duration, hashes.len());
        }
        ColorType::Rgba8 => {
            println!("This is an 8-8-8-8 RGBA image!");
            rgba_qoi(buffer, count);
        }
        _ => {
            panic!("The {colour_type:?} format is not supported by the QOI format (yet)")
        }
    }
}

const MODE: usize = 3;

fn rgba_qoi(bytes: Vec<u8>, count: usize) -> Vec<u8> {

    let safe_alloc_bytes = (count / 16 + 1) * 16; // round up to nearest multiple of 16
    /*
        Allocate 1-16 bytes extra for the hashes vector, so that writing the final __m128i doesn't
        overwrite anything that comes after it and corrupt anything. The capacity should not change,
        but the size should be set after writing everything.
     */
    let mut hashes: Vec<u8> = Vec::with_capacity(safe_alloc_bytes);

    if MODE == 1 {
        // BEGIN STUPID SIMD IMPL
        let mut pixels = bytes.array_chunks::<4>().map(|[r, g, b, a]| {
            return [*r, *g, *b, *a];
        });
        let hashing_vec: u8x4 = u8x4::from_array([3, 5, 7, 11]);

        for pixel in &mut pixels {
            let bytes: u8x4 = u8x4::from_array(pixel);
            let multiplied: u8x4 = bytes * hashing_vec;
            let hash = multiplied.reduce_sum() % 64u8;
            hashes.push(hash)
        }
    } else if MODE == 2 {
        // BEGIN SIMPLE IMPL
        let mut pixels = bytes.array_chunks::<4>().map(|[r, g, b, a]| {
            return [*r, *g, *b, *a];
        });

        for [r, g, b, a] in &mut pixels {
            let hash = ((r * 3) + (g * 5) + (b * 7) + (a * 11)) % 64;
            hashes.push(hash);
        }
    } else {
        use std::arch::asm;
        use std::arch::x86_64::__m128i;

        let rem_64: __m128i = __m128i::from(u16x8::splat(0b0000000000111111u16));
        let hash_nums: __m128i = __m128i::from(i8x16::from_array([
            3, 5, 7, 11, 3, 5, 7, 11, 3, 5, 7, 11, 3, 5, 7, 11,
        ]));

        let mut chunked_bytes = bytes.array_chunks::<64>();
        let mut full_chunk = true;
        let mut chunk: Vec<u8> = Vec::with_capacity(64);
        let pixels_ptr = chunk.as_ptr() as *const __m128i;

        let mut hashes_ptr = hashes.as_mut_ptr() as *mut __m128i;

        // BEGIN SMART HORROR

        while likely(full_chunk) {
            chunk.extend_from_slice(match chunked_bytes.next() {
                None => {
                    full_chunk = false;
                    chunked_bytes.remainder()
                }
                Some(full_chunk) => full_chunk,
            });
            unsafe {
                // cargo-asm has spoiled my perceptions of how smart the compiler really is
                // hence my nannying exactly which instructions are called

                asm!(
                    // load all the pixels into four xmm registers
                    "movdqa {a}, xmmword ptr [{pixels_ptr}]",       // get a from chunk
                    "movdqa {b}, xmmword ptr [{pixels_ptr} + 16]",  // get b from chunk
                    "movdqa {c}, xmmword ptr [{pixels_ptr} + 32]",  // get c from chunk
                    "movdqa {d}, xmmword ptr [{pixels_ptr} + 48]",  // get d from chunk

                    // hash a and b simultaneously
                    "pmaddubsw {a}, {hash_nums}",
                    "pmaddubsw {b}, {hash_nums}",
                    "phaddw {a}, {b}",                              // horizontal add
                    "pand {a}, {rem_64_mask}",                      // % 64
                    // a is now the i16x8 of the hashes of the pixels originally in a and b

                    // hash c and d simultaneously
                    "pmaddubsw {c}, {hash_nums}",
                    "pmaddubsw {d}, {hash_nums}",
                    "phaddw {c}, {d}",                              // horizontal add
                    "pand {c}, {rem_64_mask}",                      // % 64
                    // c is now the i16x8 of the hashes of the pixels originally in  c and d

                    // a becomes the final u8x16 of the 16 hashes
                    "packuswb {a}, {b}",
                    "movdqa xmmword ptr [{hashes_ptr}], {a}",       // put a into hashes

                    hash_nums = in(xmm_reg) hash_nums,
                    rem_64_mask = in(xmm_reg) rem_64,

                    pixels_ptr = in(reg) pixels_ptr,
                    hashes_ptr = in(reg) hashes_ptr,

                    // probably best to let these be set by the computer
                    a = out(xmm_reg) _,
                    b = out(xmm_reg) _,
                    c = out(xmm_reg) _,
                    d = out(xmm_reg) _

                );
                hashes_ptr = hashes_ptr.add(1);
                chunk.clear();
            }
        }
        unsafe {
            hashes.set_len(count);
        }
    }

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

    return hashes;
}
