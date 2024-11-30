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
use tokio::time::{timeout, Duration};

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


pub async fn send_image_payload_over_udp(
    socket: &UdpSocket,
    image_id: &str,
    requested_views: u16,
    buffer: Vec<u8>,
    dest_ip: &str,
    port: u16,
) -> Result<(), std::io::Error> { // Specify the Result type here
    // Prepare the JSON payload
    let response = json!({
        "type": "image_response",
        "image_id": image_id,
        "requested_views": requested_views,
        "data": base64::encode(&buffer)
    });

    let payload = serde_json::to_string(&response)?;
    let chunk_size = 1024;
    let id_size = 4;
    let data_chunk_size = chunk_size - id_size;
    let total_chunks = (payload.len() + data_chunk_size - 1) / data_chunk_size;

    for (i, chunk) in payload.as_bytes().chunks(data_chunk_size).enumerate() {
        let mut packet = Vec::with_capacity(chunk_size);
        packet.extend_from_slice(&(i as u32).to_be_bytes());
        packet.extend_from_slice(chunk);

        loop {
            socket.send_to(&packet, (dest_ip, port)).await?;
            println!("Sent chunk {}/{}", i + 1, total_chunks);

            let mut ack_buf = [0u8; 4];
            match timeout(Duration::from_secs(2), socket.recv_from(&mut ack_buf)).await {
                Ok(Ok((size, _))) if size == 4 => {
                    let ack_id = u32::from_be_bytes(ack_buf);
                    if ack_id == i as u32 {
                        println!("Received acknowledgment for chunk {}", i + 1);
                        break;
                    } else {
                        println!("Mismatched acknowledgment, retrying...");
                    }
                }
                _ => {
                    println!("No acknowledgment for chunk {}, retrying...", i + 1);
                }
            }
        }
    }

    // Send an empty packet to indicate the end of transmission
    socket.send_to(&[], (dest_ip, port)).await?;
    println!("Finished sending payload for image {}", image_id);

    Ok(())
}


pub async fn receive_image_payload_over_udp(
    socket: Arc<UdpSocket>,
) -> Result<(), Box<dyn Error>> {
    let mut received_chunks: HashMap<u32, Vec<u8>> = HashMap::new();
    let mut buffer = [0u8; 1024];
    let mut max_chunk_id = None;

    loop {
        // Receive a chunk
        let (size, src) = socket.recv_from(&mut buffer).await?;

        if size == 0 {
            println!("End of transmission signal received from {}", src);
            break;
        }

        // Extract chunk ID and data
        let chunk_id = u32::from_be_bytes(buffer[..4].try_into()?);
        let chunk_data = &buffer[4..size];
        received_chunks.insert(chunk_id, chunk_data.to_vec());

        // Send acknowledgment
        socket.send_to(&chunk_id.to_be_bytes(), &src).await?;

        // Parse chunk to check for total chunks
        if max_chunk_id.is_none() {
            if let Ok(payload) = serde_json::from_slice::<Value>(chunk_data) {
                if let Some(total_chunks) = payload.get("total_chunks").and_then(|v| v.as_u64()) {
                    max_chunk_id = Some(total_chunks as u32 - 1);
                }
            }
        }

        // If max_chunk_id is known and all chunks are received, break
        if let Some(max_id) = max_chunk_id {
            if received_chunks.len() as u32 > max_id {
                break;
            }
        }
    }

    // Reassemble the payload
    let mut complete_payload = Vec::new();
    for i in 0..=received_chunks.len() as u32 {
        if let Some(chunk) = received_chunks.get(&i) {
            complete_payload.extend_from_slice(chunk);
        } else {
            return Err(format!("Missing chunk {}", i).into());
        }
    }

    // Decode the JSON payload
    let payload: Value = serde_json::from_slice(&complete_payload)?;
    if let Some(image_id) = payload.get("image_id").and_then(|id| id.as_str()) {
        if let Some(data) = payload.get("data").and_then(|d| d.as_str()) {
            let decoded_data = base64::decode(data)?;

            // Call store_received_image
            store_received_image(image_id, &decoded_data).await?;
        } else {
            eprintln!("Payload missing 'data' field");
        }
    } else {
        eprintln!("Payload missing 'image_id' field");
    }

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
        let peer_ip = peer_addr.split(':').next().unwrap();
        let peer_port: u16 = peer_addr.split(':').nth(1).unwrap().parse()?;

        send_image_payload_over_udp(socket, image_id, requested_views, buffer, peer_ip, peer_port)
            .await?;
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
