use tokio::sync::Mutex; // Use tokio's async Mutex
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Write};
use std::net::{SocketAddr, UdpSocket as StdUdpSocket};
use std::path::Path;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration};
use rand::Rng;
use crate::encode::encode_image;

const ENCODING_IMAGE: &str = "../Encryption-images/default.jpg";

/// Attempts to allocate a unique port in the range 9000-9999, checking if it's available by binding to it temporarily.
pub async fn allocate_unique_port(used_ports: &Arc<Mutex<HashSet<u16>>>) -> io::Result<u16> {
    let mut used_ports = used_ports.lock().await;

    for port in 12348..25000{
        if used_ports.contains(&port) {
            println!("The port {} is already marked as used in the application.", port);
            continue;
        }

        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        match StdUdpSocket::bind(addr) {
            Ok(socket) => {
                drop(socket); // Release the socket to free the port
                used_ports.insert(port); // Mark the port as used
                println!("Port {} is available and allocated.", port);
                return Ok(port);
            }
            Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
                println!("Port {} is in use by another process.", port);
                continue;
            }
            Err(e) => {
                return Err(io::Error::new(io::ErrorKind::Other, format!("Failed to check port {}: {}", port, e)));
            }
        }
    }

    Err(io::Error::new(io::ErrorKind::AddrNotAvailable, "No available ports"))
}

/// Free a port after transmission completes
pub async fn free_port(used_ports: &Arc<Mutex<HashSet<u16>>>, port: u16) {
    let mut used_ports = used_ports.lock().await;
    used_ports.remove(&port);
}

/// Sends an image to the specified destination IP and port.
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

/// Saves the collected chunks to an image file in the correct order.
pub fn save_image_from_chunks(path: &str, chunks: &HashMap<usize, Vec<u8>>) -> io::Result<()> {
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

/// Receives an image over UDP, storing the chunks in a HashMap by chunk ID,
/// and returns the chunks along with the client address.
pub async fn receive_image_over_udp(socket: &UdpSocket) -> io::Result<(HashMap<usize, Vec<u8>>, SocketAddr)> {
    let mut buffer = [0u8; 1024];
    let mut chunks = HashMap::new();

    loop {
        let (size, src) = socket.recv_from(&mut buffer).await?;

        // End-of-transmission signal: check for empty packet and stop receiving
        if size == 0 {
            println!("End of transmission signal received from {}", src);
            return Ok((chunks, src));
        }

        // Handle the chunk if it's a valid data packet
        if size >= 4 {
            let chunk_id = u32::from_be_bytes(buffer[..4].try_into().unwrap());
            let chunk_data = buffer[4..size].to_vec();
            chunks.insert(chunk_id as usize, chunk_data);

            println!("Received chunk {} from {}", chunk_id, src);

            // Send acknowledgment back to the client
            socket.send_to(&chunk_id.to_be_bytes(), &src).await?;
        } else {
            println!("Received malformed or incomplete packet from {}", src);
        }
    }
}

/// Sends the encrypted image back to the client, chunked into packets.
pub async fn send_encrypted_image_back(socket: &UdpSocket, client_addr: SocketAddr, image_data: &[u8]) -> io::Result<()> {
    let chunk_size = 1024;
    let delay = Duration::from_millis(3);

    for (i, chunk) in image_data.chunks(chunk_size).enumerate() {
        socket.send_to(chunk, client_addr).await?;
        // println!("Sent encrypted chunk {} to client {}", i, client_addr);
        tokio::time::sleep(delay).await;
    }

    // Send end-of-transmission signal
    socket.send_to(&[], client_addr).await?;
    Ok(())
}

pub async fn handle_client(socket: Arc<UdpSocket>, client_addr: SocketAddr) -> io::Result<()> {
    let (chunks, _) = receive_image_over_udp(&socket).await?;

    // Save the received image
    let image_path = "../images/received_image.jpg";
    save_image_from_chunks(image_path, &chunks)?;
    println!("Image saved at {}", image_path);

    let random_id: u32 = rand::thread_rng().gen_range(1000..10000);
    let encoded_image = format!("../Encryption-images/encoded_cover{}.png", random_id);

    // Encode the image
    encode_image(ENCODING_IMAGE, image_path, &encoded_image)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    let encrypted_image_data = fs::read(&encoded_image)?;
    send_encrypted_image_back(&socket, client_addr, &encrypted_image_data).await?;
    println!("Encrypted image sent back to client {}", client_addr);

    Ok(())
}