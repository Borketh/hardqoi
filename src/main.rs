use qoi::{img_to_qoi, open_file};
mod qoi;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    println!("{:?}", args);

    let filename = &args[1];
    let image = open_file(filename);
    img_to_qoi(image)
}
