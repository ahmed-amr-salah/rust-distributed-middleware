use std::fs;
use std::io::{self, Write};
use std::path::Path;
use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration};
use serde_json::Value;
use std::net::SocketAddr;



/// Sends an image to the specified destination IP and port in chunks.
pub async fn send_image_over_udp(socket: &UdpSocket, image_path: &Path, dest_ip: String, port: u16) -> io::Result<()> {
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
            socket.send_to(&packet, (dest_ip.as_str(), port)).await?;
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
        let (size, _) = socket.recv_from(&mut buffer).await?;

        if size == 0 {
            println!("End of transmission signal received from the server.");
            break;
        }

        // Store the received data chunk
        received_chunks.push(buffer[..size].to_vec());
        println!("Received chunk with size {} bytes", size);
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


/// Multicasts a request type to all servers and retrieves the server's IP and assigned port.
///
/// # Arguments
/// - `socket`: The UDP socket bound to the client.
/// - `request_type`: A string indicating the type of request (e.g., "register", "auth").
/// - `server_ips`: A slice of server IPs to multicast to.
/// - `request_port`: The port to send the request to.
///
/// # Returns
/// The server's `SocketAddr` (IP and port) and the assigned port (`u16`).

pub async fn multicast_request_with_payload(
    socket: &UdpSocket,
    payload: String,
    server_ips: &[String],
    request_port: u16,
) -> io::Result<(std::net::SocketAddr, serde_json::Value)> {
    let mut buffer = [0u8; 1024]; // Buffer to receive the server response

    // Send the payload to all servers
    for server_ip in server_ips {
        let server_addr = format!("{}:{}", server_ip, request_port);
        socket.send_to(payload.as_bytes(), &server_addr).await?;
        println!("Sent payload to {}", server_addr);
    }

    // Wait for a response from any server
    match timeout(Duration::from_secs(7), socket.recv_from(&mut buffer)).await {
        Ok(Ok((size, src))) => {
            let response = String::from_utf8_lossy(&buffer[..size]);
            let response_json: serde_json::Value = serde_json::from_str(&response).unwrap_or_else(|_| {
                serde_json::json!({"error": "Invalid response"})
            });

            Ok((src, response_json))
        }
        _ => {
            eprintln!("No response from any server for the request.");
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "No server response for multicast request",
            ))
        }
    }
}
