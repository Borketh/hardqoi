#![feature(portable_simd, test, array_chunks, core_intrinsics)]
#[cfg(target_arch = "x86_64")]
use image::io::Reader as ImageReader;
use std::env;

use crate::qoi::{img_to_qoi, open_file};

extern crate test;
mod qoi;

fn main() {
    let args: Vec<String> = env::args().collect();
    println!("{:?}", args);

    let filename = &args[1];
    let image = open_file(filename);
    img_to_qoi(image);

}

#[cfg(test)]
mod tests {
    use super::*;
    use image::DynamicImage;
    use test::Bencher;

    #[bench]
    fn bencher(b: &mut Bencher) {
        let image: DynamicImage = open_file("test/thonk.png");
        b.iter(|| {
            println!("Get Image");
            let image = test::black_box(image.to_owned());
            println!("Do thing");
            img_to_qoi(image);
            println!("Thing done");
        });
    }
}
