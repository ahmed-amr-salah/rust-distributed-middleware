use image::{DynamicImage, GenericImageView, ImageFormat, open};
use steganography::decoder::Decoder;
use steganography::encoder::Encoder;
use std::io::Cursor;
use std::error::Error;

/// Encode the hidden image into the cover image
fn encode_image(cover_path: &str, hidden_path: &str, output_path: &str) -> Result<(), Box<dyn Error>> {
    // Load images
    let cover_img = open(cover_path)?.to_rgba();
    let hidden_img = open(hidden_path)?.to_rgba();

    // Ensure the hidden image fits within the cover image
    let (cover_width, cover_height) = cover_img.dimensions();
    let (hidden_width, hidden_height) = hidden_img.dimensions();
    assert!(hidden_width <= cover_width && hidden_height <= cover_height, "Hidden image must fit within the cover image");

    // Convert hidden image to bytes
    let mut hidden_img_bytes = Vec::new();
    DynamicImage::ImageRgba8(hidden_img).write_to(&mut Cursor::new(&mut hidden_img_bytes), ImageFormat::JPEG)?;

    // Convert cover image to DynamicImage for Encoder
    let cover_dynamic_img = DynamicImage::ImageRgba8(cover_img);

    // Initialize encoder with image data
    let encoder = Encoder::new(&hidden_img_bytes, cover_dynamic_img);
    let encoded_cover = encoder.encode_alpha();  // Use encode_alpha to hide data in alpha channel

    // Save the encoded cover image
    encoded_cover.save(output_path)?;
    Ok(())
}

/// Decode the hidden image from the encoded cover image
fn decode_image(encoded_path: &str, output_hidden_path: &str) -> Result<(), Box<dyn Error>> {
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

fn main() -> Result<(), Box<dyn Error>> {
    // Paths to images
    let cover_path = "default.jpg";
    let hidden_path = "test.jpg";
    let output_path = "encoded_cover.png";
    let decoded_hidden_path = "decoded_hidden.png";

    // Encode hidden image
    encode_image(cover_path, hidden_path, output_path)?;
    println!("Hidden image encoded successfully into {}", output_path);

    // Decode hidden image
    decode_image(output_path, decoded_hidden_path)?;
    println!("Hidden image decoded successfully to {}", decoded_hidden_path);

    Ok(())
}
