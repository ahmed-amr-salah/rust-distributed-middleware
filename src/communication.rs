use std::fs;
use std::io::{self, Write};
use std::path::Path;
use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration};

/// Sends an image to the specified destination IP and port in chunks.
pub async fn send_image_over_udp(socket: &UdpSocket, image_path: &Path, dest_ip: &str, port: u16) -> io::Result<()> {
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

    socket.send_to(&[], (dest_ip, port)).await?;
    println!("Finished sending image {:?}", image_path.file_name());

    Ok(())
}

/// Receives the encrypted image back from the server in chunks.
pub async fn receive_encrypted_image(socket: &UdpSocket, save_path: &Path) -> io::Result<()> {
    let mut buffer = [0u8; 1024];
    let mut received_chunks = Vec::new();

    println!("Waiting to receive the encrypted image from the server...");

    loop {
        let (size, _) = socket.recv_from(&mut buffer).await?;

        if size == 0 {
            println!("End of transmission signal received from the server.");
            break;
        }

        // Store the received data chunk
        received_chunks.push(buffer[..size].to_vec());
    }

    save_image_from_chunks(save_path, &received_chunks)?;
    println!("Encrypted image received and saved successfully.");

    Ok(())
}

/// Saves received chunks to a file in order.
fn save_image_from_chunks(path: &Path, chunks: &[Vec<u8>]) -> io::Result<()> {
    let mut file = fs::File::create(path)?;

    for chunk in chunks {
        file.write_all(chunk)?;
    }

    Ok(())
}
