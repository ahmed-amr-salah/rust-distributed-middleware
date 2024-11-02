use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use uuid::Uuid;
use std::fs;
use std::path::Path;

pub async fn listen_for_images(port: u16) -> io::Result<()> {
    let socket = UdpSocket::bind(("0.0.0.0", port)).await?;
    println!("Listening on port {}", port);

    let mut buffer = [0u8; 1024];
    let mut chunks = HashMap::new();
    let mut current_image_id: Option<u32> = None;

    loop {
        let (size, src) = socket.recv_from(&mut buffer).await?;

        if size == 0 {
            println!("Received end of transmission signal.");
            if let Some(image_id) = current_image_id {
                
                let dir_path = Path::new("../images");
                if !dir_path.exists() {
                    fs::create_dir_all(dir_path)?;
                }
    
                // Generate a unique filename using UUID
                let unique_filename = format!("../images/received_image_{}_{}.png", image_id, Uuid::new_v4());
                save_image(&unique_filename, &chunks)?;
                println!("Image saved as {}", unique_filename);

                // Clear the buffer for the next image
                chunks.clear();
                current_image_id = None;
            }
            continue;
        }

        // Extract the chunk ID and image ID
        if size >= 4 {
            let chunk_id = u32::from_be_bytes(buffer[..4].try_into().unwrap());

            if current_image_id.is_none() {
                current_image_id = Some(chunk_id);
                println!("Receiving image with ID: {}", chunk_id);
            }

            let chunk_data = buffer[4..size].to_vec();
            chunks.insert(chunk_id as usize, chunk_data);

            println!("Received chunk with ID: {} for image {}", chunk_id, current_image_id.unwrap());

            // Send acknowledgment back to the client
            socket.send_to(&chunk_id.to_be_bytes(), &src).await?;
        }
    }
}

/// Saves the received chunks to a file in the correct order.
fn save_image(path: &str, chunks: &HashMap<usize, Vec<u8>>) -> io::Result<()> {
    let mut file = File::create(path)?;

    for i in 0..chunks.len() {
        if let Some(chunk) = chunks.get(&i) {
            file.write_all(chunk)?;
        } else {
            println!("Warning: Missing chunk {}", i);
        }
    }

    Ok(())
}