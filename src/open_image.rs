use std::process::Command;
use std::error::Error;
use show_image::{create_window, ImageInfo, ImageView};
use image::{open, ImageBuffer, Rgb};


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