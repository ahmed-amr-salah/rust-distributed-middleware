use std::fs;
use std::io::{self, Write};
use std::path::Path;
use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration};

/// Sends an image to the specified destination IP and port in chunks.
/// Sends an image to the specified destination IP and port in chunks.
pub async fn send_image_over_udp(socket: &UdpSocket, image_path: &Path, dest_ip: String, port: u16) -> io::Result<()> {
    let image_data = fs::read(image_path)?;
    let image_name = image_path.file_name().and_then(|name| name.to_str()).unwrap_or("unknown.jpg");
    let chunk_size = 1024;
    let id_size = 4;
    let data_chunk_size = chunk_size - id_size;

    // Send the image name as the first "chunk"
    socket.send_to(image_name.as_bytes(), (dest_ip.as_str(), port)).await?;
    println!("Sent image name: {}", image_name);

    // Send the image data in chunks
    let total_chunks = (image_data.len() + data_chunk_size - 1) / data_chunk_size;

    for (i, chunk) in image_data.chunks(data_chunk_size).enumerate() {
        let mut packet = Vec::with_capacity(chunk_size);
        packet.extend_from_slice(&(i as u32).to_be_bytes()); // Add chunk ID
        packet.extend_from_slice(chunk); // Add data chunk

        loop {
            socket.send_to(&packet, (dest_ip.as_str(), port)).await?;
            println!("Sent chunk {}/{}", i + 1, total_chunks);

            let mut ack_buf = [0u8; 4];
            match timeout(Duration::from_secs(2), socket.recv_from(&mut ack_buf)).await {
                Ok(Ok((size, _))) if size == 4 => {
                    let ack_id = u32::from_be_bytes(ack_buf);
                    if ack_id == i as u32 {
                        println!("Received acknowledgment for chunk {}", i + 1);
                        break; // Acknowledgment received, proceed to the next chunk
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
    socket.send_to(&[], (dest_ip.as_str(), port)).await?;
    println!("Finished sending image {:?}", image_path.file_name());

    Ok(())
}

/// Receives the encrypted image back from the server in chunks.
pub async fn receive_encrypted_image(socket: &UdpSocket, save_path: &Path) -> io::Result<()> {
    let mut buffer = [0u8; 1024];
    let mut received_chunks = Vec::new();

    println!("Waiting to receive the encrypted image from the server...");

    loop {
        let (size, src) = socket.recv_from(&mut buffer).await?;

        if size == 0 {
            println!("End of transmission signal received from the server.");
            break;
        }

        // Parse the chunk ID from the first 4 bytes
        if size < 4 {
            eprintln!("Received malformed chunk from {}", src);
            continue;
        }
        let chunk_id = u32::from_be_bytes(buffer[..4].try_into().unwrap());
        println!("Received chunk ID {} with size {} bytes", chunk_id, size - 4);

        // Store the received data chunk (excluding the chunk ID)
        received_chunks.push(buffer[4..size].to_vec());

        // Send acknowledgment for the received chunk
        socket.send_to(&chunk_id.to_be_bytes(), src).await?;
        println!("Acknowledgment sent for chunk ID {}", chunk_id);
    }

    save_image_from_chunks(save_path, &received_chunks)?;
    println!("Encrypted image received and saved successfully.");

    Ok(())
}


/// Saves received chunks to a file in order.
fn save_image_from_chunks(path: &Path, chunks: &[Vec<u8>]) -> io::Result<()> {
    let mut file = fs::File::create(path)?;

    for (i, chunk) in chunks.iter().enumerate() {
        file.write_all(chunk)?;
        println!("Saved chunk {} to file", i + 1);
    }

    Ok(())
}
