use std::process::Command;
use std::error::Error;

/// Open an image with the system's default viewer
// pub fn open_image_with_default_viewer(image_path: &str) -> Result<(), Box<dyn std::error::Error>> {
//     if cfg!(target_os = "windows") {
//         Command::new("explorer")
//             .arg(image_path)
//             .spawn()
//             .expect("Failed to open image on Windows");
//     } else if cfg!(target_os = "macos") {
//         Command::new("open")
//             .arg(image_path)
//             .spawn()
//             .expect("Failed to open image on macOS");
//     } else if cfg!(target_os = "linux") {
//         Command::new("xdg-open")
//             .arg(image_path)
//             .spawn()
//             .expect("Failed to open image on Linux");
//     } else {
//         return Err("Unsupported operating system".into());
//     }
//     Ok(())
// }



// use show_image::{create_window, ImageInfo, ImageView};

// pub fn open_image_with_default_viewer(image_path: &str) {
//     if let Ok(img) = image::open(image_path) {
//         let img = img.to_rgb(); // Convert to RGB format
//         let (width, height) = img.dimensions();
        
//         let window = create_window("Image Viewer", Default::default()).unwrap();

//         let image_info = ImageInfo::rgb8(width, height);
//         let image_view = ImageView::new(image_info, &img);
//         window.set_image("Image", image_view).unwrap();
//     } else {
//         println!("Failed to load image.");
//     }
// }


// use show_image::{Context, ImageInfo, ImageView};
// use image::open;

// pub fn open_image_with_default_viewer(image_path: &str) {
//     // Initialize the global context
//     let context = Context::new().expect("Failed to initialize show-image context");

//     // Open the image
//     if let Ok(img) = open(image_path) {
//         let img = img.to_rgb(); // Convert to RGB8 format
//         let (width, height) = img.dimensions();

//         // Create a window to display the image
//         let window = context
//             .create_window("Image Viewer", Default::default())
//             .expect("Failed to create window");

//         // Create an ImageView and set the image
//         let image_info = ImageInfo::rgb8(width , height );
//         let image_view = ImageView::new(image_info, img.as_flat_samples().as_slice());
//         window.set_image("Image", image_view).expect("Failed to set image");

//         // Wait until the window is closed
//         window.wait_until_closed().expect("Failed to wait for window close");
//     } else {
//         println!("Failed to load image.");
//     }
// }


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