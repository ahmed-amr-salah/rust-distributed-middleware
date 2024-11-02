// use std::fs;
// use std::io;
// use std::net::UdpSocket;

// pub fn send_image_over_udp(image_path: &str, dest_ip: &str, port: u16) -> io::Result<()> {
//     // Bind to any available port for sending
//     let socket = UdpSocket::bind("0.0.0.0:0")?;
//     let image_data = fs::read(image_path)?;  // Read the entire image file into a byte vector

//     let chunk_size = 1024;
//     let id_size = 4; // Size of the ID in bytes
//     let data_chunk_size = chunk_size - id_size; // Adjust for the ID space
//     let total_chunks = (image_data.len() + data_chunk_size - 1) / data_chunk_size;

//     for (i, chunk) in image_data.chunks(data_chunk_size).enumerate() {
//         // Create a buffer to hold ID + chunk
//         let mut packet = Vec::with_capacity(chunk_size);

//         // Add the ID to the packet (as 4 bytes)
//         packet.extend_from_slice(&(i as u32).to_be_bytes());
        
//         // Add the actual chunk data
//         packet.extend_from_slice(chunk);

//         // Send packet with ID
//         socket.send_to(&packet, (dest_ip, port))?;
//         println!("Sending chunk {}/{}", i + 1, total_chunks);
//     }

//     // Optionally send a zero-size packet to signal end of transmission
//     socket.send_to(&[], (dest_ip, port))?;

//     Ok(())
// }

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Write};  // Import io and Write for error handling and write_all
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::time::{sleep, Duration};

pub async fn send_directory_images(directory_path: &str, dest_ip: &str, port: u16, n: usize) -> io::Result<()> {
    let image_files: Vec<_> = fs::read_dir(directory_path)?
        .filter_map(Result::ok)
        .filter(|entry| is_image_file(&entry.path()))
        .collect();

    if image_files.is_empty() {
        println!("No images found in the directory: {}", directory_path);
        return Ok(());
    }

    println!("Found {} images in directory. Sending each image {} times.", image_files.len(), n);

    let server_addr = format!("{}:{}", dest_ip, port);
    let server_addr: SocketAddr = server_addr.parse().unwrap();

    for _ in 0..n {
        for entry in &image_files {
            let image_path = entry.path();
            let image_name = image_path.file_name().unwrap().to_string_lossy().into_owned(); // Extract name before moving

            let server_addr = server_addr.clone();
            let image_path_clone = image_path.clone();
            tokio::spawn(async move {
                if let Err(e) = send_image_over_udp(&image_path_clone, &server_addr).await {
                    eprintln!("Failed to send image {:?}: {}", image_path_clone.file_name(), e);
                }
            });

            // Spawn a thread to listen for the server's reply
            let reply_socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
            let reply_socket_clone = Arc::clone(&reply_socket);

            std::thread::spawn(move || {
                tokio::runtime::Runtime::new().unwrap().block_on(async move {
                    listen_for_reply(reply_socket_clone, &image_name).await.unwrap();
                });
            });
        }
    }

    Ok(())
}

/// Sends an image over UDP with reliability.
pub async fn send_image_over_udp(image_path: &Path, dest_addr: &SocketAddr) -> io::Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let image_data = fs::read(image_path)?;

    let chunk_size = 1024;
    let id_size = 4;
    let data_chunk_size = chunk_size - id_size;
    let total_chunks = (image_data.len() + data_chunk_size - 1) / data_chunk_size;

    for (i, chunk) in image_data.chunks(data_chunk_size).enumerate() {
        let mut packet = Vec::with_capacity(chunk_size);

        packet.extend_from_slice(&(i as u32).to_be_bytes());
        packet.extend_from_slice(chunk);

        loop {
            if let Err(e) = socket.send_to(&packet, dest_addr).await {
                eprintln!("Error sending chunk {}: {}", i + 1, e);
                return Err(e);
            }
            println!("Sending chunk {}/{} of image {:?}", i + 1, total_chunks, image_path.file_name());

            let mut ack_buf = [0u8; 4];
            match tokio::time::timeout(Duration::from_secs(2), socket.recv_from(&mut ack_buf)).await {
                Ok(Ok((size, _))) if size == 4 => {
                    let ack_id = u32::from_be_bytes(ack_buf);
                    if ack_id == i as u32 {
                        println!("Received acknowledgment for chunk {}", i + 1);
                        break; // Proceed to the next chunk
                    } else {
                        println!("Received mismatched acknowledgment, retrying chunk {}", i + 1);
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("Error receiving acknowledgment for chunk {}: {}", i + 1, e);
                    return Err(e);
                }
                _ => {
                    println!("No acknowledgment for chunk {}, retrying...", i + 1);
                    sleep(Duration::from_millis(500)).await; // Pause briefly before retrying
                }
            }
        }
    }

    // Send an empty packet to signal end of transmission
    if let Err(e) = socket.send_to(&[], dest_addr).await {
        eprintln!("Error sending end-of-transmission signal: {}", e);
        return Err(e);
    }
    Ok(())
}

/// Listens for the server's reply, saves the received image, then closes the thread.
async fn listen_for_reply(socket: Arc<UdpSocket>, image_name: &str) -> io::Result<()> {
    let mut buffer = [0u8; 1024];
    let mut chunks = HashMap::new();

    loop {
        let (size, _) = socket.recv_from(&mut buffer).await?;
        if size == 0 {
            println!("End of transmission for reply image {}", image_name);
            let reply_filename = format!("reply_{}", image_name);
            save_image(&reply_filename, &chunks)?;
            println!("Saved reply as {}", reply_filename);
            break;
        }

        let chunk_id = u32::from_be_bytes(buffer[..4].try_into().unwrap());
        let chunk_data = buffer[4..size].to_vec();
        chunks.insert(chunk_id as usize, chunk_data);
    }

    Ok(())
}

/// Saves received chunks to a file.
fn save_image(path: &str, chunks: &HashMap<usize, Vec<u8>>) -> io::Result<()> {
    let mut file = File::create(path)?;
    for i in 0..chunks.len() {
        if let Some(chunk) = chunks.get(&i) {
            file.write_all(chunk)?;
        }
    }
    Ok(())
}

/// Helper function to check if a file is an image.
fn is_image_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some(ext) if ["png", "jpg", "jpeg", "gif", "bmp"].contains(&ext.to_lowercase().as_str())
    )
}


// use std::fs;
// use std::io;
// use std::net::SocketAddr;
// use std::path::Path;
// use tokio::net::UdpSocket;
// use tokio::time::{sleep, timeout, Duration};

// pub async fn send_image_over_udp(image_path: &Path, dest_addr: &SocketAddr) -> io::Result<()> {
//     let socket = UdpSocket::bind("0.0.0.0:0").await?;
//     let image_data = fs::read(image_path)?;

//     let chunk_size = 1024;
//     let id_size = 4;
//     let data_chunk_size = chunk_size - id_size;
//     let total_chunks = (image_data.len() + data_chunk_size - 1) / data_chunk_size;

//     for (i, chunk) in image_data.chunks(data_chunk_size).enumerate() {
//         let mut packet = Vec::with_capacity(chunk_size);

//         packet.extend_from_slice(&(i as u32).to_be_bytes());
//         packet.extend_from_slice(chunk);

//         // Retry loop for sending the packet and waiting for acknowledgment
//         loop {
//             println!("Client: Sending chunk {}/{} of image {:?}", i + 1, total_chunks, image_path.file_name());

//             if let Err(e) = socket.send_to(&packet, dest_addr).await {
//                 eprintln!("Client: Error sending chunk {}: {}", i + 1, e);
//                 return Err(e);
//             }

//             let mut ack_buf = [0u8; 4];
//             match timeout(Duration::from_secs(2), socket.recv_from(&mut ack_buf)).await {
//                 Ok(Ok((size, _))) if size == 4 => {
//                     let ack_id = u32::from_be_bytes(ack_buf);
//                     if ack_id == i as u32 {
//                         println!("Client: Received acknowledgment for chunk {}", i + 1);
//                         break; // Acknowledgment received, proceed to the next chunk
//                     } else {
//                         println!("Client: Mismatched acknowledgment ID for chunk {}, retrying...", i + 1);
//                     }
//                 }
//                 Ok(Err(e)) => {
//                     eprintln!("Client: Error receiving acknowledgment for chunk {}: {}", i + 1, e);
//                     return Err(e);
//                 }
//                 Err(_) => {
//                     println!("Client: No acknowledgment for chunk {}, retrying...", i + 1);
//                 }
//             }

//             sleep(Duration::from_millis(500)).await; // Brief pause before retrying
//         }
//     }

//     // Send an empty packet to signal end of transmission
//     socket.send_to(&[], dest_addr).await?;
//     println!("Client: End of transmission signal sent");

//     Ok(())
// }

/// Helper function to check if a file is an image based on its extension.
fn is_image_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some(ext) if ["png", "jpg", "jpeg", "gif", "bmp"].contains(&ext.to_lowercase().as_str())
    )
}
