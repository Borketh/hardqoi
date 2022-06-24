use image::io::Reader as ImageReader;
use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::io::Cursor;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();
    println!("{:?}", args);

    let filename = &args[1];

    let img = ImageReader::open("myimage.png")?.decode()?;
    let img2 = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()?
        .decode()?;
    // Create a path to the desired file
    let path = Path::new(filename);
    let display = path.display();

    // Open the path in read-only mode, returns `io::Result<File>`
    let file = match File::open(&path) {
        Err(why) => panic!("couldn't open {}: {}", display, why),
        Ok(file) => file,
    };

    let mut reader = BufReader::new(file);
    let mut buffer = Vec::new();

    // Read file into vector.
    match reader.read_to_end(&mut buffer) {
        Ok(_) => {}
        Err(why) => panic!("Oh heck! {:?}", why),
    }

    // Read.
    for value in buffer {
        println!("BYTE: {}", value);
    }
    // `file` goes out of scope, and gets closed
}
