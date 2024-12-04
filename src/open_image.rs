use std::process::Command;
use std::error::Error;
use show_image::{create_window, ImageInfo, ImageView};
use image::{open, ImageBuffer, Rgb, DynamicImage, Rgba, RgbaImage};

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

// pub fn view_cover_image_without_hidden_data(cover_image_path: &str) -> Result<(), Box<dyn std::error::Error>> {
//     // Open the cover image (which has the hidden image encoded in it)
//     let cover_image = image::open(cover_image_path)?;
    
//     // If the image is RGBA (with an alpha channel), we can ignore the alpha channel (hidden data)
//     if let DynamicImage::ImageRgba8(rgba_image) = cover_image {
//         let (width, height) = rgba_image.dimensions();
        
//         // Create a new RGB image (without the alpha channel)
//         let mut rgb_image: RgbaImage = RgbaImage::new(width, height);

//         for (x, y, pixel) in rgba_image.enumerate_pixels() {
//             let (r, g, b, _a) = *pixel;  // We ignore the alpha channel (_a)
//             rgb_image.put_pixel(x, y, Rgba([r, g, b, 255]));  // Put only the RGB values
//         }

//         // Save or display the image without the hidden data (alpha channel)
//         rgb_image.save("cover_without_hidden_data.png")?;
//         println!("Saved cover image without hidden data.");

//         Ok(())
//     } else {
//         Err("The cover image is not RGBA.".into())
//     }
// }