use image::{DynamicImage, GenericImageView, ImageFormat, open, imageops::FilterType};
use steganography::encoder::Encoder;
use std::error::Error;
use std::io::Cursor;

pub fn encode_image(cover_path: &str, hidden_path: &str, output_path: &str) -> Result<(), Box<dyn Error>> {
    println!("Starting encode_image function");
    println!("Cover path: {}", cover_path);
    println!("Hidden path: {}", hidden_path);
    println!("Output path: {}", output_path);

    // Load cover image
    let cover_img = match open(cover_path) {
        Ok(img) => {
            println!("Successfully loaded cover image.");
            img.to_rgba()
        },
        Err(e) => {
            eprintln!("Error loading cover image: {}", e);
            return Err(Box::new(e));
        }
    };

    // Load hidden image
    let mut hidden_img = match open(hidden_path) {
        Ok(img) => {
            println!("Successfully loaded hidden image.");
            img.to_rgba()
        },
        Err(e) => {
            eprintln!("Error loading hidden image: {}", e);
            return Err(Box::new(e));
        }
    };

    // Ensure the hidden image fits within the cover image
    let (cover_width, cover_height) = cover_img.dimensions();
    let (hidden_width, hidden_height) = hidden_img.dimensions();
    println!("Cover image dimensions: {}x{}", cover_width, cover_height);
    println!("Hidden image dimensions: {}x{}", hidden_width, hidden_height);

    if hidden_width > cover_width || hidden_height > cover_height {
        println!("Resizing hidden image to fit within cover image dimensions.");
        hidden_img = image::imageops::resize(&hidden_img, cover_width, cover_height, FilterType::Lanczos3);
        println!("Resized hidden image dimensions: {}x{}", cover_width, cover_height);
    }

    // Convert hidden image to bytes
    let mut hidden_img_bytes = Vec::new();
    DynamicImage::ImageRgba8(hidden_img).write_to(&mut Cursor::new(&mut hidden_img_bytes), ImageFormat::JPEG)?;

    // Convert cover image to DynamicImage for Encoder
    let cover_dynamic_img = DynamicImage::ImageRgba8(cover_img);

    // Initialize encoder with image data
    let encoder = Encoder::new(&hidden_img_bytes, cover_dynamic_img);
    let encoded_cover = encoder.encode_alpha();
    println!("Encoding completed using encode_alpha.");

    // Save the encoded cover image
    if let Err(e) = encoded_cover.save(output_path) {
        eprintln!("Error saving encoded cover image: {}", e);
        return Err(Box::new(e));
    }
    println!("Encoded image saved successfully at {}", output_path);

    Ok(())
}