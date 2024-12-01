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
use std::io;
use std::collections::HashMap;
use serde_json::Value;
use tokio::io::AsyncWriteExt;

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

    //LISTEN TO REQUESTED IMAGE

    receive_encrypted_image_from_client(socket).await?;

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

    println!("IN ENCODE: num_views = {}", num_views);

    // Get the byte representation of `num_views`
    let binary_data = num_views.to_be_bytes();

    // Encode the byte data directly into the image
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

    // Ensure that we have at least 2 bytes of data
    if decoded_data.len() < 2 {
        return Err("Decoded data is too short".into());
    }

    // Convert the first 2 bytes back into a `u16` value
    let decoded_value = u16::from_be_bytes([decoded_data[0], decoded_data[1]]);

    println!("DECODED: num_views = {}", decoded_value);
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
        // "type": "image_response",
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

pub async fn receive_encrypted_image_from_client(socket: &UdpSocket) -> io::Result<()> {
    let mut buffer = [0u8; 1024];
    let mut received_chunks = Vec::new();

    println!("RECEIVED_ENCRYPTED_IMAGE_IS_CALLED");

    println!("Waiting to receive the encrypted image payload...");

    loop {
        // Receive a packet
        let (size, src_addr) = socket.recv_from(&mut buffer).await?;

        // If an empty packet is received, it signals the end of the transmission
        if size == 0 {
            println!("End of transmission signal received from the server.");
            break;
        }
        // Handle directly sent payload (non-chunked JSON payload)
        if let Ok(payload_str) = std::str::from_utf8(&buffer[..size]) {
            if let Ok(response) = serde_json::from_str::<serde_json::Value>(payload_str) {
                // Check if it's a direct JSON payload
                if let Some(response_type) = response["type"].as_str() {
                    match response_type {
                        "image_rejection" => {
                            let image_id = response["image_id"]
                                .as_str()
                                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Missing image ID"))?;
                            let requested_views = response["views"]
                                .as_u64()
                                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Missing views"))?;
                            println!(
                                "Received rejection response for image '{}' with requested views: {}",
                                image_id, requested_views
                            );
                            // Prepare acknowledgment payload
                            let rejection_ack = serde_json::json!({
                                "type": "rejection_ack",
                                "status": "received",
                                "image_id": image_id
                            });

                            let rejection_ack_string = rejection_ack.to_string(); // Store the string in a variable
                            let rejection_ack_bytes = rejection_ack_string.as_bytes(); // Use the stored value here
                            socket.send_to(rejection_ack_bytes, src_addr).await?;

                            println!(
                                "Acknowledgment sent for image_rejection with image_id: {} to {}",
                                image_id, src_addr
                            );
                            return Ok(()); // Exit early since no image chunks will be processed
                        }
                        _ => {
                            println!("Received unhandled direct payload type: {}", response_type);
                            return Ok(());
                        }
                    }
                }
            }
        }

        // Extract the chunk ID and payload from the packet
        if size >= 4 {
            let chunk_id = u32::from_be_bytes(buffer[..4].try_into().unwrap());
            let data_chunk = &buffer[4..size];
            println!("Received chunk ID: {} with size {} bytes", chunk_id, data_chunk.len());

            // Store the received data chunk
            while received_chunks.len() <= chunk_id as usize {
                received_chunks.push(Vec::new());
            }
            received_chunks[chunk_id as usize] = data_chunk.to_vec();

            // Send acknowledgment for the received chunk
            socket.send_to(&chunk_id.to_be_bytes(), src_addr).await?;
            println!("Acknowledgment sent for chunk ID: {}", chunk_id);
        } else {
            println!("Received malformed packet with size {}", size);
        }
    }

    // Combine all chunks to reconstruct the payload
    let payload = received_chunks.concat();
    let payload_str = String::from_utf8(payload).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    // Parse the JSON payload
    let response: serde_json::Value = serde_json::from_str(&payload_str)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    // Extract image ID and data from the payload
    let image_id = response["image_id"].as_str()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing image ID"))?;
    let encoded_data = response["data"].as_str()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing image data"))?;

    // Decode the base64 image data
    let image_data = base64::decode(encoded_data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    // Call the `store_received_image` function to handle saving the image and metadata
    if let Err(err) = store_received_image(image_id, &image_data).await {
        println!("Failed to store image {}: {}", image_id, err);
        return Err(std::io::Error::new(std::io::ErrorKind::Other, "Failed to store received image"));
    }

    println!("Encrypted image received and processed successfully.");

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
    _socket: &UdpSocket,
    image_id: &str,
    requested_views: u16,
    peer_addr: &str,
    approved: bool
) -> Result<(), Box<dyn std::error::Error>> {
    // Create a new socket bound to an ephemeral port
    let responder_socket = UdpSocket::bind("0.0.0.0:0").await?;
    let responder_local_addr = responder_socket.local_addr()?;
    println!(
        "Responder bound to new socket at {}",
        responder_local_addr
    );
    if approved {
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

        println!("In respond_to_request: {}:{}", peer_ip , peer_port);

        send_image_payload_over_udp(&responder_socket, image_id, requested_views, buffer, peer_ip, peer_port)
            .await?;
        println!("Approved request for {} views", requested_views);
    } else {
        let response = json!({
            "type": "image_rejection",
            "views": requested_views,
            "image_id": image_id
        });

        responder_socket.send_to(response.to_string().as_bytes(), peer_addr).await?;
        println!("Rejected request for image {}", image_id);
    }
    Ok(())
}


pub async fn respond_to_increase_views(
    socket: &UdpSocket,
    image_id: &str,
    requested_views: u16,
    peer_addr: &str,
    approved: bool
) -> Result<(), Box<dyn std::error::Error>> {

    if approved {
        let response = json!({
            "type": "increase_approved",
            "views" : requested_views,
            "image_id": image_id
        });
        socket.send_to(response.to_string().as_bytes(), peer_addr).await?;
    }
    else {
        let response = json!({
            "type": "increase_rejected",
            "views" : requested_views,
            "image_id": image_id
        });
        socket.send_to(response.to_string().as_bytes(), peer_addr).await?;
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

    // Update the JSON file with the image_id and its views
    let json_file_path = dir.join("images_views.json");
    let mut image_views: HashMap<String, u32> = if fs::metadata(&json_file_path).await.is_ok() {
        // File exists; read and parse it
        let json_content = fs::read_to_string(&json_file_path).await?;
        serde_json::from_str(&json_content).unwrap_or_default()
    } else {
        // File does not exist; start with an empty map
        HashMap::new()
    };

    // Update or insert the image_id with the decoded views
    image_views
        .entry(image_id.to_string())
        .and_modify(|views| *views += decoded_views as u32)
        .or_insert(decoded_views as u32);

    // Write the updated JSON back to the file
    let updated_json = serde_json::to_string_pretty(&image_views)?;
    fs::write(&json_file_path, updated_json).await?;

    println!(
        "Stored image {} with {} views (updated total in JSON).",
        image_id, decoded_views
    );

    Ok(())
}

pub async fn handle_increase_views_response(image_id: &str, extra_views: u32, approved: bool) -> Result<(), Box<dyn std::error::Error>> {
    // Path to the JSON file storing the views
    let json_file_path = Path::new("image_views.json");

    // Check if the request is approved
    if approved {
        // Read the JSON file content
        let mut views_data: Value = if json_file_path.exists() {
            let file_content = fs::read_to_string(json_file_path).await?;
            serde_json::from_str(&file_content)?
        } else {
            // Create a new JSON object if the file doesn't exist
            Value::Object(serde_json::Map::new())
        };

        // Get the current views for the given image ID
        let current_views = views_data
            .get(image_id)
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        // Calculate the new views
        let new_views = current_views + extra_views as u64;

        // Update the JSON with the new views
        if let Value::Object(map) = &mut views_data {
            map.insert(image_id.to_string(), Value::Number(new_views.into()));
        }

        // Write the updated JSON back to the file
        let mut file = fs::File::create(json_file_path).await?;
        file.write_all(serde_json::to_string_pretty(&views_data)?.as_bytes()).await?;

        println!("Successfully updated views for image '{}'. New views count: {}", image_id, new_views);
    } else {
        println!("Increase views request for image '{}' was rejected for {} views.", image_id, extra_views);
    }

    Ok(())
}