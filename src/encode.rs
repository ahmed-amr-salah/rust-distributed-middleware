use image::{DynamicImage, ImageFormat, open};
use steganography::encoder::Encoder;
use std::error::Error;
use std::io::Cursor;

/// Encode the hidden image into the cover image
pub fn encode_image(cover_path: &str, hidden_path: &str, output_path: &str) -> Result<(), Box<dyn Error>> {
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
