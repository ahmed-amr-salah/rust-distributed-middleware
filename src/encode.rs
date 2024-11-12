use image::{DynamicImage, ImageFormat, open, imageops::FilterType};
use steganography::encoder::Encoder;
use std::error::Error;
use std::io::Cursor;
use tokio::task;

pub async fn encode_image_async(cover_path: String, hidden_path: String, output_path: String) -> Result<(), Box<dyn Error + Send + Sync>> {
    println!("Starting async encode_image function");

    task::spawn_blocking(move || {
        println!("Cover path: {}", cover_path);
        println!("Hidden path: {}", hidden_path);
        println!("Output path: {}", output_path);

        // Load cover image
        let cover_img = open(&cover_path).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?.to_rgba();

        // Load hidden image
        let mut hidden_img = open(&hidden_path).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?.to_rgba();

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
        DynamicImage::ImageRgba8(hidden_img).write_to(&mut Cursor::new(&mut hidden_img_bytes), ImageFormat::JPEG)
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        // Convert cover image to DynamicImage for Encoder
        let cover_dynamic_img = DynamicImage::ImageRgba8(cover_img);

        // Initialize encoder with image data
        let encoder = Encoder::new(&hidden_img_bytes, cover_dynamic_img);
        let encoded_cover = encoder.encode_alpha();
        println!("Encoding completed using encode_alpha.");

        // Save the encoded cover image
        encoded_cover.save(&output_path).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        println!("Encoded image saved successfully at {}", output_path);

        Ok::<(), Box<dyn Error + Send + Sync>>(()) // Provide `()` as the argument to `Ok`
    })
    .await?
}
