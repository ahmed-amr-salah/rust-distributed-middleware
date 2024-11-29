use tokio::fs;
use std::path::Path;
use serde_json::json;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};
use std::sync::Arc;
use steganography::encoder::Encoder;
use steganography::decoder::Decoder;
use image::{open, DynamicImage};
use std::error::Error;
use std::io::Cursor;
use image::ImageOutputFormat;
use image::ImageFormat;
use tokio::task;
use crate::config::load_config;
/// Sends an image request to another peer.
///
/// # Arguments
/// - `socket`: The UDP socket used to send the request.
/// - `peer_addr`: The address of the peer.
/// - `image_id`: The ID of the image being requested.
/// - `requested_views`: The number of views being requested.
pub async fn send_image_request(
    socket: &UdpSocket,
    peer_addr: &str,
    image_id: &str,
    requested_views: u16,
) -> Result<(), std::io::Error> { // Updated return type
    let request = json!({
        "type": "image_request",
        "image_id": image_id,
        "views": requested_views
    });

    socket.send_to(request.to_string().as_bytes(), peer_addr).await?;
    println!("Sent image request to {}", peer_addr);
    Ok(())
}

// Async encode function
// Async encode function
pub async fn encode_access_rights(
    img_id: &str,
    ip_address: &str,
    num_views: u16,
) -> Result<DynamicImage, Box<dyn Error>> {
    let config = load_config();

    let image_path = format!("{}/encrypted_{}.png", config.save_dir.display(), img_id);

    // Read and load the image file asynchronously
    let image_data = fs::read(&image_path).await?;
    let image = task::spawn_blocking(move || {
        image::load_from_memory_with_format(&image_data, ImageFormat::PNG) // Explicitly specify format
    })
    .await??;

    let binary_data = num_views.to_be_bytes();

    // Encode the binary data
    let encoder = Encoder::new(&binary_data, image);
    let encoded_image = encoder.encode_alpha();

    Ok(DynamicImage::ImageRgba8(encoded_image))
}

pub async fn decode_access_rights(
    encoded_image: DynamicImage,
) -> Result<u16, Box<dyn std::error::Error>> {
    // Perform decoding in a blocking task
    let decoded_data = tokio::task::spawn_blocking(move || {
        let decoder = Decoder::new(encoded_image.to_rgba());
        decoder.decode_alpha()
    })
    .await?;

    // Convert the binary data to a u16 value
    let decoded_value = decoded_data
        .iter()
        .take(16)
        .fold(0, |acc, &bit| (acc << 1) | bit as u16);

    Ok(decoded_value)
}

/// Handles incoming image requests.
///
/// # Arguments
/// - `socket`: The UDP socket used to respond.
/// - `image_id`: The ID of the requested image.
/// - `available_views`: The number of views available.
/// - `peer_addr`: The address of the requesting peer.
pub async fn respond_to_request(
    socket: &UdpSocket,
    image_id: &str,
    requested_views: u16,
    peer_addr: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if requested_views < 10 {

        let encoded_img = encode_access_rights(image_id, peer_addr, requested_views).await?;
        let buffer = tokio::task::spawn_blocking(move || {
            let mut buffer = Vec::new();
            encoded_img
                .write_to(&mut Cursor::new(&mut buffer), ImageOutputFormat::PNG)
                .expect("Failed to write image to buffer");
            buffer // Return the buffer
        })
        .await?;
        let response = json!({
            "type": "image_response",
            "image_id": image_id,
            "requested_views": requested_views,
            "data": base64::encode(&buffer) // Replace with actual encrypted data
        });

        socket.send_to(response.to_string().as_bytes(), peer_addr).await?;
        println!("Approved request for {} views", requested_views);
    } else {
        let response = json!({
            "type": "image_rejection",
            "reason": "Not enough rights or unavailable"
        });

        socket.send_to(response.to_string().as_bytes(), peer_addr).await?;
        println!("Rejected request for image {}", image_id);
    }
    Ok(())
}

/// Stores received images and metadata.
///
/// # Arguments
/// - `image_id`: The ID of the image being stored.
/// - `data`: The encrypted image data.
/// - `views`: The number of views allowed.
pub async fn store_received_image(
    image_id: &str,
    data: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = Path::new("../Peer_Images");

    // Create directory for storing images
    tokio::fs::create_dir_all(dir).await?;

    // Save the encrypted image asynchronously
    let file_path = dir.join(format!("{}_encrypted.png", image_id));
    tokio::fs::write(&file_path, data).await?;

    // Clone data to pass it into the blocking task
    let data_owned = data.to_vec();
    let encoded_image = tokio::task::spawn_blocking(move || {
        image::load_from_memory_with_format(&data_owned, ImageFormat::PNG)
    })
    .await??;

    // Decode the access rights asynchronously
    let decoded_views = decode_access_rights(encoded_image).await?;

    // Save metadata asynchronously
    let metadata_path = dir.join(format!("{}_meta.json", image_id));
    let metadata = json!({
        "views": decoded_views
    });
    tokio::fs::write(metadata_path, metadata.to_string()).await?;

    println!(
        "Stored image {} with {} views",
        image_id, decoded_views
    );

    Ok(())
}
