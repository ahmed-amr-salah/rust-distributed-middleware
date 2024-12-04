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

use crate::communication;
use crate::decode;
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
    let num_views_bytes = num_views.to_be_bytes();

    // Expand the image to add a new row
    let mut rgba_image = image.to_rgba();
    let (width, height) = rgba_image.dimensions();

    // Create a new image buffer with one additional row
    let mut new_image = image::ImageBuffer::new(width, height + 1);

    // Copy the original image data
    for y in 0..height {
        for x in 0..width {
            new_image.put_pixel(x, y, *rgba_image.get_pixel(x, y));
        }
    }

    // Add the `num_views` data to the new row
    for (x, byte) in num_views_bytes.iter().enumerate() {
        if x as u32 >= width {
            break; // Avoid exceeding image width
        }
        // Encode the byte as a pixel's alpha channel
        new_image.put_pixel(
            x as u32,
            height,
            image::Rgba([0, 0, 0, *byte]), // Encoding data in the alpha channel
        );
    }

    // Convert the buffer back to a DynamicImage
    let encoded_image = DynamicImage::ImageRgba8(new_image);

    Ok(encoded_image)
}


pub fn decode_access_rights(
    encoded_image: DynamicImage,
) -> Result<(u16, DynamicImage), Box<dyn std::error::Error>> {
    // Convert the image to RGBA for processing
    let mut rgba_image = encoded_image.to_rgba();
    let (width, height) = rgba_image.dimensions();

    if height < 1 {
        return Err("Invalid image: no rows to decode".into());
    }

    // Extract the last row, which contains the `num_views` data
    let last_row_y = height - 1;
    let mut num_views_bytes = [0u8; 2];
    for x in 0..2 {
        if x as u32 >= width {
            break; // Avoid exceeding image width
        }
        let pixel = rgba_image.get_pixel(x as u32, last_row_y);
        num_views_bytes[x] = pixel[3]; // Extract the alpha channel
    }

    // Decode the `num_views` (u16) from the bytes
    let num_views = u16::from_be_bytes(num_views_bytes);

    // Remove the last row from the image
    let mut original_image = image::ImageBuffer::new(width, height - 1);
    for y in 0..(height - 1) {
        for x in 0..width {
            original_image.put_pixel(x, y, *rgba_image.get_pixel(x, y));
        }
    }

    // Convert back to DynamicImage
    let original_image = DynamicImage::ImageRgba8(original_image);

    Ok((num_views, original_image))
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
    approved: bool,
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

        println!("In respond_to_request: {}:{}", peer_ip, peer_port);

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

        // Wait for "rejection_ack" acknowledgment
        let mut ack_buf = [0u8; 1024];
        match timeout(Duration::from_secs(5), responder_socket.recv_from(&mut ack_buf)).await {
            Ok(Ok((size, _))) => {
                if let Ok(ack_json) = serde_json::from_slice::<serde_json::Value>(&ack_buf[..size]) {
                    if ack_json.get("type") == Some(&serde_json::Value::String("rejection_ack".to_string()))
                        && ack_json.get("image_id") == Some(&serde_json::Value::String(image_id.to_string()))
                    {
                        println!("Received 'rejection_ack' for image '{}'", image_id);
                    } else {
                        println!(
                            "Received acknowledgment, but it is not a valid 'rejection_ack': {:?}",
                            ack_json
                        );
                    }
                } else {
                    println!("Failed to parse acknowledgment as JSON.");
                }
            }
            Ok(Err(e)) => {
                eprintln!("Error receiving acknowledgment: {}", e);
            }
            Err(_) => {
                println!("Timed out waiting for 'rejection_ack' acknowledgment.");
            }
        }
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
    let response = if approved {
        json!({
            "type": "increase_approved",
            "views": requested_views,
            "image_id": image_id
        })
    } else {
        json!({
            "type": "increase_rejected",
            "views": requested_views,
            "image_id": image_id
        })
    };

    println!("I WILL BE WAITING FOR ACK ON {}", socket.local_addr().unwrap());

    socket.send_to(response.to_string().as_bytes(), peer_addr).await?;
    println!(
        "Sent response to {}: {}",
        peer_addr,
        if approved { "increase_approved" } else { "increase_rejected" }
    );

    // Wait for acknowledgment
    let mut ack_buf = [0u8; 1024];
    match timeout(Duration::from_secs(10), socket.recv_from(&mut ack_buf)).await {
        Ok(Ok((size, src_addr))) => {
            if let Ok(ack_json) = serde_json::from_slice::<serde_json::Value>(&ack_buf[..size]) {
                let ack_type = if approved {
                    "increase_approved_ack"
                } else {
                    "increase_rejected_ack"
                };

                if ack_json.get("type") == Some(&serde_json::Value::String(ack_type.to_string()))
                    && ack_json.get("image_id") == Some(&serde_json::Value::String(image_id.to_string()))
                {
                    println!("Received '{}' acknowledgment from {}", ack_type, src_addr);
                } else {
                    println!(
                        "Received acknowledgment, but it is not a valid '{}': {:?}",
                        ack_type, ack_json
                    );
                }
            } else {
                println!("Failed to parse acknowledgment as JSON.");
            }
        }
        Ok(Err(e)) => {
            eprintln!("Error receiving acknowledgment: {}", e);
        }
        Err(_) => {
            if approved {
            // Multicast shutdown request
            let update_view_response = json!({
                "type": "change-view",
                "image_id": image_id,
                "requested_views": requested_views,
                "peer_address": peer_addr
            });
            let config = load_config();

            let (server_addr, shutdown_response) =
                communication::multicast_request_with_payload(
                    &socket,
                    update_view_response.to_string(),
                    &config.server_ips,
                    config.request_port,
                )
                .await?;

            println!("Sent change-view request to {}", server_addr);
            println!("change-view response: {}", shutdown_response);
        }
            // println!(
            //     "Timed out waiting for '{}' acknowledgment for image '{}'.",
            //     if approved { "increase_approved_ack" } else { "increase_rejected_ack" },
            //     image_id
            // );
        }
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

    // Decode the access rights
    let (decoded_views, first_layer_image) = match decode_access_rights(encoded_image) {
        Ok(result) => result,
        Err(e) => {
            println!("Failed to decode access rights: {}", e);
            return Err(e); // Propagate the error if needed
        }
    };

    // Save the first-layer encoded image
    let first_layer_file_path = dir.join(format!("{}_first_layer.png", image_id));
    tokio::task::spawn_blocking({
        let first_layer_file_path = first_layer_file_path.clone(); // Clone to avoid move issues
        move || first_layer_image.save(&first_layer_file_path)
    })
    .await??;

    // // Decode the hidden image from the first-layer image
    // let hidden_file_path = dir.join(format!(".{}.png", image_id));
    // tokio::task::spawn_blocking({
    //     let first_layer_file_path = first_layer_file_path.clone(); // Clone to avoid move issues
    //     let hidden_file_path = hidden_file_path.clone();           // Clone to avoid move issues
    //     move || {
    //         decode::decode_image(
    //             first_layer_file_path.to_str().unwrap(),
    //             hidden_file_path.to_str().unwrap(),
    //         )
    //     }
    // })
    // .await?;

    // println!(
    //     "Hidden image successfully extracted and saved at: {}",
    //     hidden_file_path.display()
    // );

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
    let json_file_path = Path::new("../Peer_Images/images_views.json");

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


pub async fn handle_update_access_rights(
    image_id: &str,
    extra_views: u32,
) -> Result<(), Box<dyn Error>> {
    // Path to the JSON file storing the views
    let json_file_path = Path::new("../Peer_Images/images_views.json");

    //erroneus
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

    Ok(())
}