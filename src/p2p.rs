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
// use crate::decode::decode_image;
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
) -> Result<DynamicImage, Box<dyn std::error::Error>> {
    let config = load_config();

    let image_path = format!("{}/encrypted_{}.png", config.save_dir.display(), img_id);

    // Read and load the image file asynchronously
    let image_data = fs::read(&image_path).await?;
    let mut image = tokio::task::spawn_blocking(move || {
        image::load_from_memory_with_format(&image_data, ImageFormat::PNG) // Explicitly specify format
    })
    .await??;

    println!("IN ENCODE: num_views = {}", num_views);

    // Get the byte representation of `num_views`
    let binary_data = num_views.to_be_bytes();

    // Reserve a specific region at the bottom-left corner
    let mut image_rgba = image.to_rgba8();
    let (width, height) = (image_rgba.width(), image_rgba.height());

    // Ensure the image is large enough to encode the data
    if binary_data.len() > 4 || height < 1 {
        return Err("Image too small or data too large to encode.".into());
    }

    // Embed the access rights data in the bottom-left corner
    for (i, &byte) in binary_data.iter().enumerate() {
        let x = 0; // Always the left-most column
        let y = height - 1 - i as u32; // Bottom-left, moving upwards for each byte
        if y >= height {
            return Err("Not enough space in the bottom-left region.".into());
        }
        let pixel = image_rgba.get_pixel_mut(x, y);
        pixel[3] = byte; // Store in the alpha channel (or another channel if preferred)
    }

    // Convert back to DynamicImage
    let encoded_image = DynamicImage::ImageRgba8(image_rgba);

    Ok(encoded_image)
}



/// Remove the access rights encoding and return the intermediate image
pub fn strip_access_rights(encoded_image: DynamicImage) -> Result<DynamicImage, Box<dyn std::error::Error>> {
    // Convert to RGBA8 (4-channel format)
    let mut rgba_image = encoded_image.to_rgba();

    // Clear the first two bytes of the alpha channel where the access rights are stored
    for (i, pixel) in rgba_image.pixels_mut().enumerate() {
        if i < 2 {
            // Reset the alpha value of the first two pixels
            pixel[3] = 255; // Fully opaque (default alpha)
        } else {
            break;
        }
    }

    // Return the modified image as a `DynamicImage`
    Ok(DynamicImage::ImageRgba8(rgba_image))
}


// pub async fn decode_access_rights(
//     encoded_image: DynamicImage,
//     image_id: &str,
// ) -> Result<u16, Box<dyn std::error::Error>> {

//     let encoded_image_clone = encoded_image.clone();

//     // Step 1: Decode the access rights (upper layer of encoding)
//     let decoded_data = tokio::task::spawn_blocking(move || {
//         let decoder = Decoder::new(encoded_image_clone.to_rgba());
//         decoder.decode_alpha()
//     })
//     .await?;

//     // Ensure there are at least 2 bytes for decoding access rights
//     if decoded_data.len() < 2 {
//         return Err("Decoded data is too short".into());
//     }

//     // Convert the first 2 bytes to a `u16` value representing the access rights
//     let decoded_value = u16::from_be_bytes([decoded_data[0], decoded_data[1]]);
//     println!("DECODED: num_views = {}", decoded_value);

//     // Step 2: Strip the access rights layer to get the intermediate image
//     let intermediate_image = strip_access_rights(encoded_image)?;

//     // Step 3: Save the intermediate image to a temp folder
//     let temp_directory = "../Peer_Images"; // Temporary folder for intermediate storage
//     let temp_image_path = format!("{}/{}_intermediate.png", temp_directory, image_id); // Temporary file path
//     encoded_image.save(&temp_image_path)?; // Save the intermediate image

//     // Step 4: Decode the lower layer (hidden image) using `decode_image`
//     let peer_images_directory = "../Peer_Images"; // Final output directory
//     let hidden_file_path = format!("{}/.{}.png", peer_images_directory, image_id); // Hidden raw image path

//     // Decode the hidden image and save it as a hidden file
//     // decode_image(&temp_image_path, &hidden_file_path);
//     println!("Starting the decoding process...");
    
//     // Load the encoded cover image
//     let encoded_dynamic_img = open(temp_image_path)?;
//     println!("[Signature: Load Image] - Successfully loaded encoded image.");

//     // Convert DynamicImage to ImageBuffer for Decoder
//     let encoded_img = encoded_dynamic_img.to_rgba();
//     println!("[Signature: Convert Image] - Conversion successful.");

//     // Initialize decoder with the encoded image buffer
//     let decoder = Decoder::new(encoded_img);
//     println!("[Signature: Initialize Decoder] - Decoder initialized successfully.");
//     let hidden_img_bytes = decoder.decode_alpha();

//     // Convert the decoded bytes back into an image and save it
//     println!("Before loading from memory");
//     let hidden_img = match image::load_from_memory_with_format(&hidden_img_bytes, image::ImageFormat::PNG) {
//         Ok(img) => img,
//         Err(e) => {
//             println!("Failed to load hidden image: {:?}", e);
//             return Err(Box::new(e));
//         }
//     };    
//     println!("After loading from memory");
//     hidden_img.save(hidden_file_path.clone())?;
//     // println!("Image saved successfully in {}", hidden_file_path);



//     // Step 6: Clean up the temporary file
//     // std::fs::remove_file(&temp_image_path)?;

//     println!("Hidden raw image saved to: {}", hidden_file_path);

//     Ok(decoded_value)
// }

pub async fn decode_access_rights(
    encoded_image: DynamicImage,
) -> Result<(u16, DynamicImage), Box<dyn std::error::Error>> {
    let mut image_rgba = encoded_image.to_rgba8();
    let (width, height) = (image_rgba.width(), image_rgba.height());

    // Ensure the image has enough height to decode the data
    if height < 2 {
        return Err("Image too small to decode access rights.".into());
    }

    // Read the access rights data from the bottom-left corner
    let mut binary_data = [0u8; 2];
    for (i, byte) in binary_data.iter_mut().enumerate() {
        let x = 0; // Always the left-most column
        let y = height - 1 - i as u32; // Bottom-left, moving upwards for each byte
        if y >= height {
            return Err("Not enough data in the bottom-left region.".into());
        }
        let pixel = image_rgba.get_pixel_mut(x, y);
        *byte = pixel[3]; // Read from the alpha channel

        // Remove the encoded access rights by resetting the alpha channel
        pixel[3] = 255; // Set to fully opaque
    }

    // Convert back to u16
    let decoded_value = u16::from_be_bytes(binary_data);
    println!("DECODED: num_views = {}", decoded_value);

    // Convert the modified RGBA image back to a DynamicImage
    let modified_image = DynamicImage::ImageRgba8(image_rgba);

    Ok((decoded_value, modified_image))
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

    // let decrypted_image_data = base64::decode::decode_image()


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


                                // // Multicast shutdown request
                                // let shutdown_json = json!({
                                //     "type": "shutdown",
                                //     "user_id": user_id,
                                //     "randam_number": random_num,
                                // });

                                // let (server_addr, shutdown_response) =
                                //     communication::multicast_request_with_payload(
                                //         &socket,
                                //         shutdown_json.to_string(),
                                //         &config.server_ips,
                                //         config.request_port,
                                //     )
                                //     .await?;

                                // println!("Sent shutdown request to {}", server_addr);
                                // println!("Shutdown response: {}", shutdown_response);
                                // break;


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
            // if approved {

            // }
            // // Multicast shutdown request
            // let update_view_response = json!({
            //     "type": "change-view",
            //     "image_id": image_id,
            //     "requested_views": requested_views,
            //     "peer_address": peer_addr
            // });

            // let (server_addr, shutdown_response) =
            //     communication::multicast_request_with_payload(
            //         &socket,
            //         shutdown_json.to_string(),
            //         &config.server_ips,
            //         config.request_port,
            //     )
            //     .await?;

            // println!("Sent shutdown request to {}", server_addr);
            // println!("Shutdown response: {}", shutdown_response);
            // break;
            println!(
                "Timed out waiting for '{}' acknowledgment for image '{}'.",
                if approved { "increase_approved_ack" } else { "increase_rejected_ack" },
                image_id
            );
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

    // Save the original encrypted image asynchronously
    let encrypted_file_path = dir.join(format!("{}_encrypted.png", image_id));
    tokio::fs::write(&encrypted_file_path, data).await?;

    // Clone data to pass it into the blocking task
    let data_owned = data.to_vec();
    let encoded_image = tokio::task::spawn_blocking(move || {
        image::load_from_memory_with_format(&data_owned, ImageFormat::PNG)
    })
    .await??;

    // Decode the access rights asynchronously
    let (decoded_views, encoded_image_1) = decode_access_rights(encoded_image).await?;

    // Save the modified image (encoded_image_1) to a separate file
    let cleaned_image_path = dir.join(format!("{}_cleaned.png", image_id));
    tokio::task::spawn_blocking(move || {
        encoded_image_1.save_with_format(cleaned_image_path, ImageFormat::PNG)
    })
    .await??;

    // Call the decode_image function to extract the original hidden image
    let hidden_image_path = dir.join(format!("{}_original.png", image_id));
    decode::decode_image(
        encrypted_file_path.to_str().unwrap(),
        hidden_image_path.to_str().unwrap(),
    )?;
    println!("Hidden image decoded and saved to {}", hidden_image_path.display());

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