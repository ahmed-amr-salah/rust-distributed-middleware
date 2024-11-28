use std::fs;
use std::path::Path;
use serde_json::json;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};
use std::sync::Arc;

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
    requested_views: u32,
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

/// Stores received images and metadata.
///
/// # Arguments
/// - `image_id`: The ID of the image being stored.
/// - `data`: The encrypted image data.
/// - `views`: The number of views allowed.
pub fn store_received_image(image_id: &str, data: &[u8], views: u32) -> Result<(), std::io::Error> {
    let dir = Path::new("../Peer_Images");
    fs::create_dir_all(dir)?;

    let file_path = dir.join(format!("{}_encrypted", image_id));
    let metadata_path = dir.join(format!("{}_meta.json", image_id));

    fs::write(file_path, data)?;
    let metadata = json!({ "views": views });
    fs::write(metadata_path, metadata.to_string())?;

    println!("Stored image {} with {} views", image_id, views);
    Ok(())
}

