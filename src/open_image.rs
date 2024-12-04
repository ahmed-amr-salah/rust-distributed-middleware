use std::process::Command;
use std::error::Error;
use show_image::{create_window, ImageInfo, ImageView};
use image::{open, ImageBuffer, Rgb};
use tempfile::TempDir;
use std::path::PathBuf;

pub async fn open_image_with_default_viewer(image_path: &str) -> Result<(), Box<dyn Error>> {
    // Step 1: Open the image
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = open(image_path)?.to_rgb();

    // Step 2: Get the image dimensions
    let (width, height) = img.dimensions();
    let binding = img.as_flat_samples();
    let raw_pixels = binding.as_slice();

    // Step 3: Create a window to display the image
    let window = create_window("Image Viewer", Default::default())?;

    // Step 4: Create an ImageView and set the image
    let image_info = ImageInfo::rgb8(width, height);
    let image_view = ImageView::new(image_info, raw_pixels);
    window.set_image("Image", image_view)?;

    // Step 5: Wait until the window is closed
    window.wait_until_destroyed()?;
    Ok(())
}

pub fn create_temp_hidden_file(image_id: String) -> std::io::Result<PathBuf> {
    // Create a temporary directory
    let temp_dir = TempDir::new()?;

    // Construct the hidden file path in the temporary directory
    let hidden_file_path = temp_dir.path().join(format!(".{}.png", image_id));

    // Return the path to the hidden file
    Ok(hidden_file_path)
}