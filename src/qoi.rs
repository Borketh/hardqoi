use crate::ImageReader;
use image::{ColorType, DynamicImage, GenericImageView};

use std::path::Path;
use std::time::Instant;

static MOD_64: u128 = 0x003f003f003f003f003f003f003f003fu128;
static HASH_NUMS: u128 = 0x0b0705030b0705030b0705030b070503u128;

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
    let length = {
        let dims = image.dimensions();
        dims.0 * dims.1
    } as usize;
    raw_to_qoi(image.into_rgba8().into_raw(), colour_type, length);
}

fn raw_to_qoi(bytes: Vec<u8>, colour_type: ColorType, count: usize) {
    match colour_type {
        ColorType::Rgb8 | ColorType::Rgba8=> {
            println!("This is an RGB/RGBA image!");
            let timekeeper = Instant::now();
            let hashes = HASHES_RGBA(&bytes, count);
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
        }
        _ => {
            panic!("The {colour_type:?} format is not supported by the QOI format (yet)")
        }
    }
}

fn hashes_rgba_dispatch() -> fn(&Vec<u8>, usize) -> Vec<u8> {
    if std::is_x86_feature_detected!("ssse3") {
        hashes_rgba_x86_ssse3
    } else {
        hashes_rgba_naive
    }
}

lazy_static! {
    static ref HASHES_RGBA: fn(&Vec<u8>, usize) -> Vec<u8> = hashes_rgba_dispatch();
}

// don't use this one
fn _hashes_rgba_weird_asm(bytes: &Vec<u8>, count: usize) -> Vec<u8> {
    use std::simd::u8x4;

    let safe_iterations = count / 16 + 1;
    let safe_alloc_bytes = safe_iterations * 16;
    // Allocate 1-16 bytes extra for the hashes vector, so that writing the final __m128i doesn't
    // overwrite anything that comes after it and corrupt anything. The capacity should not change,
    // but the size should be set after writing everything.
    let mut hashes: Vec<u8> = Vec::with_capacity(safe_alloc_bytes);

    let mut pixels = bytes.array_chunks::<4>().map(|[r, g, b, a]| {
        return [*r, *g, *b, *a];
    });
    let hashing_vec: u8x4 = u8x4::from_array([3, 5, 7, 11]);

    for pixel in &mut pixels {
        let bytes: u8x4 = u8x4::from_array(pixel);
        let multiplied: u8x4 = bytes * &hashing_vec;
        let hash = multiplied.reduce_sum() % 64u8;
        hashes.push(hash)
    }
    return hashes;
}

fn hashes_rgba_naive(bytes: &Vec<u8>, count: usize) -> Vec<u8> {
    let safe_iterations = count / 16 + 1;
    let safe_alloc_bytes = safe_iterations * 16;
    // Allocate 1-16 bytes extra for the hashes vector, so that writing the final __m128i doesn't
    // overwrite anything that comes after it and corrupt anything. The capacity should not change,
    // but the size should be set after writing everything.
    let mut hashes: Vec<u8> = Vec::with_capacity(safe_alloc_bytes);

    let mut pixels = bytes.array_chunks::<4>().map(|[r, g, b, a]| {
        return [*r, *g, *b, *a];
    });

    for [r, g, b, a] in &mut pixels {
        let hash = ((r * 3) + (g * 5) + (b * 7) + (a * 11)) % 64;
        hashes.push(hash);
    }
    return hashes;
}

#[inline]
fn hashes_rgba_x86_ssse3(bytes: &Vec<u8>, count: usize) -> Vec<u8> {
    // this wraps the "unsafe" enclosed function to make the function pointer type equivalent
    // to other implementations of hashes_rgba
    unsafe { hashes_rgba_ssse3_assembly(bytes, count) }
}

#[target_feature(enable = "ssse3")]
unsafe fn hashes_rgba_ssse3_assembly(bytes: &Vec<u8>, count: usize) -> Vec<u8> {
    use std::arch::asm;
    use std::arch::x86_64::__m128i;

    let safe_iterations = count / 16 + 1;
    let safe_alloc_bytes = safe_iterations * 16;
    // Allocate 1-16 bytes extra for the hashes vector, so that writing the final __m128i doesn't
    // overwrite anything that comes after it and corrupt anything. The capacity should not change,
    // but the size should be set after writing everything.
    let mut hashes: Vec<u8> = Vec::with_capacity(safe_alloc_bytes);

    let mut pixels_ptr: *const __m128i = bytes.as_ptr() as *const __m128i;
    let mut hashes_ptr = hashes.as_mut_ptr() as *mut __m128i;
    let mod_64_ptr: *const u128 = &MOD_64;
    let hash_nums_ptr: *const u128 = &HASH_NUMS;

    // reserve xmm0 and xmm1 for quick access of the hashing numbers and mod mask

    asm!(
    "movdqu xmm0, xmmword ptr [{hash_nums_ptr}]",
    "movdqu xmm1, xmmword ptr [{mod_64_ptr}]",
    hash_nums_ptr = in(reg) hash_nums_ptr,
    mod_64_ptr = in(reg) mod_64_ptr,

    out("xmm0") _,
    out("xmm1") _,
    options(readonly, preserves_flags)
    );

    for _ in 0..safe_iterations {
        // cargo-asm has spoiled my perceptions of how smart the compiler really is
        // hence my nannying exactly which instructions are called

        asm!(
        // load 16 pixels into four xmm registers
        "movdqa {a}, xmmword ptr [{pixels_ptr}]",         // get a from chunk
        "movdqa {b}, xmmword ptr [{pixels_ptr} + 16]",    // get b from chunk
        "movdqa {c}, xmmword ptr [{pixels_ptr} + 32]",    // get c from chunk
        "movdqa {d}, xmmword ptr [{pixels_ptr} + 48]",    // get d from chunk

        // hash a and b simultaneously
        "pmaddubsw {a}, xmm0",
        "pmaddubsw {b}, xmm0",
        "phaddw {a}, {b}",                                  // horizontal add
        "pand {a}, xmm1",                                   // % 64
        // a is now the i16x8 of the hashes of the pixels originally in a and b

        // hash c and d simultaneously
        "pmaddubsw {c}, xmm0",
        "pmaddubsw {d}, xmm0",
        "phaddw {c}, {d}",                                  // horizontal add
        "pand {c}, xmm1",                                   // % 64
        // c is now the i16x8 of the hashes of the pixels originally in  c and d

        // a becomes the final u8x16 of the 16 hashes
        "packuswb {a}, {b}",
        "movntdq xmmword ptr [{hashes_ptr}], {a}",          // put a into hashes

        pixels_ptr = in(reg) pixels_ptr,
        hashes_ptr = in(reg) hashes_ptr,

        // probably best to let these be set by the compiler
        a = out(xmm_reg) _,
        b = out(xmm_reg) _,
        c = out(xmm_reg) _,
        d = out(xmm_reg) _,
        out("xmm0") _, // reserved for hashing numbers
        out("xmm1") _, // reserved for mod 64 mask

        options(preserves_flags)
        );

        hashes_ptr = hashes_ptr.add(1);
        pixels_ptr = pixels_ptr.add(4);
    }

    hashes.set_len(count);

    return hashes;
}
