use std::fs::{self, File};
use std::path::Path;
use serde_json::json;
use tokio::net::UdpSocket;

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

// Handles incoming image requests
pub async fn respond_to_request(
    socket: &UdpSocket,
    image_id: &str,
    available_views: u32,
    peer_addr: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let approved_views = available_views.min(3); // Example logic: Approve up to 3 views
    if approved_views > 0 {
        let response = json!({
            "type": "image_response",
            "image_id": image_id,
            "approved_views": approved_views,
            "data": "encrypted_image_data" // Replace with actual encrypted data
        });

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
