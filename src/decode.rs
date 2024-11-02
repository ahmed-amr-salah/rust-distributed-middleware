use image::open;
use steganography::decoder::Decoder;
use std::error::Error;

/// Decode the hidden image from the encoded cover image
pub fn decode_image(encoded_path: &str, output_hidden_path: &str) -> Result<(), Box<dyn Error>> {
    // Load the encoded cover image
    let encoded_dynamic_img = open(encoded_path)?;

    // Convert DynamicImage to ImageBuffer for Decoder
    let encoded_img = encoded_dynamic_img.to_rgba();

    // Initialize decoder with the encoded image buffer
    let decoder = Decoder::new(encoded_img);
    let hidden_img_bytes = decoder.decode_alpha();  // Use decode_alpha to retrieve data from alpha channel

    // Convert the decoded bytes back into an image and save it
    let hidden_img = image::load_from_memory(&hidden_img_bytes)?;
    hidden_img.save(output_hidden_path)?;
    Ok(())
}
