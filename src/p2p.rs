use std::fs::{self, File};
use std::path::Path;
use serde_json::json;
use tokio::net::UdpSocket;
use steganography::encoder::Encoder;
use steganography::decoder::Decoder;
use image::{open, DynamicImage};
use std::error::Error;
use std::fs;

// Sends an image request to another peer
pub async fn send_image_request(
    socket: &UdpSocket,
    peer_addr: &str,
    image_id: &str,
    requested_views: u32,
) -> Result<(), Box<dyn std::error::Error>> {
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
pub async fn encode_access_rights(
    img_id: &str,
    ip_address: &str,
    num_views: u16,
) -> Result<DynamicImage, Box<dyn Error>> {
    // Convert IP address and number of views to binary data
    let mut binary_data = Vec::with_capacity(48); // Pre-allocate space for 32 bits of IP + 16 bits of views
    binary_data.extend(ip_to_binary(ip_address));
    binary_data.extend(num_views_to_binary(num_views));

    // Load the cover image asynchronously
    let img_path = img_id + ".png";
    let image = tokio::task::spawn_blocking(move || {
        open(img_path)?.to_rgba()
    })
    .await??;

    // Create the encoder and encode the data into the image
    let encoder = Encoder::new(&binary_data, DynamicImage::ImageRgba8(cover_image));
    let encoded_image = tokio::task::spawn_blocking(move || {
        encoder.encode_alpha()
    })
    .await??;

    // Wrap the encoded image buffer in a DynamicImage
    Ok(DynamicImage::ImageRgba8(encoded_image))
}

fn decode_access_rights(
    encoded_image: &DynamicImage,
    output_file_path: &str,
) -> Result<(), Box<dyn Error>> {
    // Create the decoder and decode the data
    let decoder = Decoder::new(encoded_image.to_rgba());
    let decoded_data = decoder.decode_alpha();

    // Extract IP address and number of views from the decoded data
    let ip_binary = &decoded_data[..32];
    let views_binary = &decoded_data[32..48];

    let ip_address = ip_binary.chunks(8)
    .map(|chunk| chunk.iter().fold(0, |acc, &bit| (acc << 1) | bit) as u8)
    .map(|octet| octet.to_string())
    .collect::<Vec<String>>()
    .join(".");

    let num_views = views_binary.iter().fold(0, |acc, &bit| (acc << 1) | bit as u16);

    // Save the decoded data to a text file
    let decoded_text = format!("Decoded IP Address: {}\nDecoded Number of Views: {}", ip_address, num_views);
    fs::write(output_file_path, decoded_text)?;

    println!("Decoded access rights saved to {}", output_file_path);
    Ok(())
}

// Handles incoming image requests
pub async fn respond_to_request(
    socket: &UdpSocket,
    image_id: &str,
    available_views: u32,
    peer_addr: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let approved_views = available_views.min(3); // Example logic: Approve up to 3 views

    let encoded_img = encode_access_rights(image_id, peer_addr, approved_views); 
    if approved_views > 0 {
        // Serialize the encoded image to PNG
        let mut buffer = Vec::new();
        tokio::task::spawn_blocking({
            let encoded_image = encoded_image.clone();
            move || encoded_image.write_to(&mut Cursor::new(&mut buffer), ImageOutputFormat::Png)
        })
        .await??;

        let response = json!({
            "type": "image_response",
            "image_id": image_id,
            "approved_views": approved_views,
            "data": base64::encode(&buffer)// Replace with actual encrypted data
        });
        let peer_addr = socket.local_addr()?;

        socket.send_to(response.to_string().as_bytes(), peer_addr).await?;
        println!("Approved request for {} views", approved_views);
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

// Stores received images and metadata
pub fn store_received_image(image_id: &str, data: &[u8], views: u32) -> Result<(), std::io::Error> {
    let dir = Path::new("Peer_Images");
    fs::create_dir_all(dir)?;

    let file_path = dir.join(format!("{}_encrypted", image_id));
    let metadata_path = dir.join(format!("{}_meta.json", image_id));

    fs::write(file_path, data)?;
    let metadata = json!({ "views": views });
    fs::write(metadata_path, metadata.to_string())?;

    println!("Stored image {} with {} views", image_id, views);
    Ok(())
}


// Decrypts and displays an image
pub fn decrypt_and_display_image(image_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let dir = Path::new("Peer_Images");
    let file_path = dir.join(format!("{}_encrypted", image_id));
    let metadata_path = dir.join(format!("{}_meta.json", image_id));

    if !file_path.exists() || !metadata_path.exists() {
        return Err("Image or metadata not found".into());
    }

    let metadata: serde_json::Value = serde_json::from_str(&fs::read_to_string(metadata_path)?)?;
    let remaining_views = metadata["views"].as_u64().unwrap_or(0);

    if remaining_views == 0 {
        println!("No views left for image {}", image_id);
        fs::remove_file(file_path)?;
        fs::remove_file(metadata_path)?;
        return Ok(());
    }

    let decrypted_data = "decrypted_image_data"; // Replace with actual decryption logic
    let hidden_path = Path::new("Hidden_Images").join(format!("{}_decrypted", image_id));
    fs::write(&hidden_path, decrypted_data)?;

    // Simulate pop-up display (Replace with actual pop-up logic)
    println!("Displaying image {}", hidden_path.display());

    // Decrement views and update metadata
    let updated_metadata = json!({ "views": remaining_views - 1 });
    fs::write(metadata_path, updated_metadata.to_string())?;

    if updated_metadata["views"].as_u64() == Some(0) {
        fs::remove_file(file_path)?;
        println!("Deleted image {} after final view", image_id);
    }

    Ok(())
}
