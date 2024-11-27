use tokio::sync::Mutex; // Use tokio's async Mutex
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Write};
use std::net::{SocketAddr, UdpSocket as StdUdpSocket};
use std::path::Path;
use std::sync::Arc;
use std::error::Error;
use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration};
use mysql_async::{prelude::*, Pool};
use mysql_async::Conn;

use rand::Rng;
use crate::encode::encode_image;

const ENCODING_IMAGE: &str = "/home/g6/Desktop/Hamza's_Work/dist4/rust-distributed-middleware/Encryption-images/default.jpg";

/// Attempts to allocate a unique port in the range 9000-9999, checking if it's available by binding to it temporarily.
pub async fn allocate_unique_port(used_ports: &Arc<Mutex<HashSet<u16>>>) -> io::Result<u16> {
    let mut used_ports = used_ports.lock().await;

    for port in 12348..25000 {
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

/// Free a port after transmission completes.
pub async fn free_port(used_ports: &Arc<Mutex<HashSet<u16>>>, port: u16) {
    let mut used_ports = used_ports.lock().await;
    used_ports.remove(&port);
}

/// Receives an image or a special message over UDP, storing the chunks or handling the message.
pub async fn receive_image_over_udp(socket: &UdpSocket) -> io::Result<(Option<String>, HashMap<usize, Vec<u8>>, SocketAddr)> {
    let mut buffer = [0u8; 1024];
    let mut chunks = HashMap::new();

    // Step 1: Check the first received packet
    let (size, src) = socket.recv_from(&mut buffer).await?;
    let message = String::from_utf8_lossy(&buffer[..size]).to_string();

    if message == "CLIENT_OFFLINE" {
        println!("Received CLIENT_OFFLINE message from {}", src);
        return Ok((None, chunks, src)); // Special case: No image name, just handle offline message
    }

    // Step 2: Treat it as an image name otherwise
    println!("Received image name: {} from {}", message, src);

    // Step 3: Receive the image chunks
    loop {
        let (size, src) = socket.recv_from(&mut buffer).await?;

        if size == 0 {
            println!("End of transmission signal received from {}", src);
            break;
        }

        if size >= 4 {
            let chunk_id = u32::from_be_bytes(buffer[..4].try_into().unwrap());
            let chunk_data = buffer[4..size].to_vec();
            chunks.insert(chunk_id as usize, chunk_data);

            println!("Received chunk {} from {}", chunk_id, src);

            socket.send_to(&chunk_id.to_be_bytes(), &src).await?;
            println!("Acknowledged chunk ID: {} to {}", chunk_id, src);
        } else {
            println!("Received malformed or incomplete packet from {}", src);
        }
    }

    Ok((Some(message), chunks, src))
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

pub async fn check_and_add(
    conn: &mut Conn,
    client_ip: &str,
    image_id: &str,
) -> Result<(), Box<dyn Error>> {
    // Check if client exists
    let exists: Option<String> = conn.exec_first(
        "SELECT client_ip FROM client WHERE client_ip = :client_ip",
        params! { "client_ip" => client_ip },
    ).await?;

    if let Some(_) = exists {
        // Only update resources if image_id is not empty
        if !image_id.is_empty() {
            conn.exec_drop(
                "INSERT IGNORE INTO resources (client_ip, image_id) VALUES (:client_ip, :image_id)",
                params! { "client_ip" => client_ip, "image_id" => image_id },
            ).await?;
            println!("Client {} exists; updated resources with image_id: {}", client_ip, image_id);
        }
    } else {
        // Add client and resource
        conn.exec_drop(
            "INSERT INTO client (client_ip, is_up) VALUES (:client_ip, true)",
            params! { "client_ip" => client_ip },
        ).await?;
        println!("Added client {}", client_ip);

        if !image_id.is_empty() {
            conn.exec_drop(
                "INSERT INTO resources (client_ip, image_id) VALUES (:client_ip, :image_id)",
                params! { "client_ip" => client_ip, "image_id" => image_id },
            ).await?;
            println!("Added resource for client {} with image_id: {}", client_ip, image_id);
        }
    }
    Ok(())
}

pub async fn shutdown_client(conn: &mut Conn, client_ip: &str) -> Result<(), Box<dyn Error>> {
    conn.exec_drop(
        "UPDATE client SET is_up = false WHERE client_ip = :client_ip",
        params! { "client_ip" => client_ip },
    ).await?;
    println!("Client {} marked as offline.", client_ip);
    Ok(())
}

/// Handles client interaction: receives images, updates database, and sends encrypted images.
pub async fn handle_client(
    socket: Arc<UdpSocket>,
    client_addr: SocketAddr,
    db_pool: Arc<Pool>,
) -> io::Result<()> {
    let client_ip = client_addr.ip().to_string();
    let mut conn = db_pool.get_conn().await.map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("Database connection error: {}", e))
    })?;

    check_and_add(&mut conn, &client_ip, "").await.map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("Database query failed: {}", e))
    })?;

    loop {
        let image_dir = "/home/g6/Desktop/Hamza's_Work/dist4/rust-distributed-middleware/received_images/";
        let encoded_image_path = "/home/g6/Desktop/Hamza's_Work/dist4/rust-distributed-middleware/Encryption-images/encoded_image.png";

        let (maybe_image_name, chunks, src) = receive_image_over_udp(&socket).await?;
        if maybe_image_name.is_none() {
            // Handle client offline message
            shutdown_client(&mut conn, &client_ip).await.map_err(|e| {
                io::Error::new(io::ErrorKind::Other, format!("Database query failed: {}", e))
            })?;
            println!("Client {} marked as offline.", client_ip);
            break;
        }

        let image_name = maybe_image_name.unwrap();
        println!("Received {} chunks for image: {}", chunks.len(), image_name);

        let image_stem = Path::new(&image_name)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("unknown");

        let image_extension = Path::new(&image_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");

        let unique_image_id = if image_extension.is_empty() {
            format!("{}_{}", image_stem, client_ip)
        } else {
            format!("{}_{}.{}", image_stem, client_ip, image_extension)
        };

        println!("Generated unique image ID: {}", unique_image_id);

        let received_image_path = format!("{}{}", image_dir, unique_image_id);
        save_image_from_chunks(&received_image_path, &chunks)?;

        check_and_add(&mut conn, &client_ip, &unique_image_id).await.map_err(|e| {
            io::Error::new(io::ErrorKind::Other, format!("Database query failed: {}", e))
        })?;

        encode_image(ENCODING_IMAGE, &received_image_path, encoded_image_path)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        send_image_over_udp(&socket, Path::new(encoded_image_path), &client_ip, client_addr.port()).await?;
    }

    Ok(())
}

pub async fn handle_client_exit(client_addr: SocketAddr, db_pool: Arc<Pool>) -> io::Result<()> {
    let client_ip = client_addr.ip().to_string();
    let mut conn = db_pool.get_conn().await.map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("Database connection error: {}", e))
    })?;
    shutdown_client(&mut conn, &client_ip).await.map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("Database query failed: {}", e))
    })?;
    println!("Client {} disconnected.", client_ip);
    Ok(())
}
