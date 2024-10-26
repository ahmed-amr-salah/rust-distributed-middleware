mod encode;
mod decode;

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    // Paths to images
    let cover_path = "default.jpg";
    let hidden_path = "test.jpg";
    let output_path = "encoded_cover.png";
    let decoded_hidden_path = "decoded_hidden.png";

    // Encode hidden image
    encode::encode_image(cover_path, hidden_path, output_path)?;
    println!("Hidden image encoded successfully into {}", output_path);

    // Decode hidden image
    decode::decode_image(output_path, decoded_hidden_path)?;
    println!("Hidden image decoded successfully to {}", decoded_hidden_path);

    Ok(())
}
