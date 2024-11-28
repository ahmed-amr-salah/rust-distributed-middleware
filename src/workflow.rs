use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{timeout, Duration};
use crate::communication;
use crate::p2p;
use crate::config::Config;

/// Determines which server will handle the request for a given resource ID.
///
/// # Arguments
/// - `socket`: A reference to the bound `UdpSocket`.
/// - `server_ips`: A slice of server IPs.
/// - `request_port`: The port to send the request.
/// - `resource_id`: A unique identifier for the resource.
///
/// # Returns
/// The server's address and assigned port if successful, otherwise `None`.
pub async fn find_server_for_resource(
    socket: &UdpSocket,
    server_ips: &[String],
    request_port: u16,
    resource_id: &str,
) -> io::Result<Option<(String, u16)>> {
    let mut buffer = [0u8; 2];

    // Send the resource ID to all servers
    for server_ip in server_ips {
        let server_addr = format!("{}:{}", server_ip, request_port);
        socket.send_to(resource_id.as_bytes(), &server_addr).await?;
        println!("Sent resource ID {} to server at {}", resource_id, server_addr);
    }

    // Wait for a response
    for _ in 0..server_ips.len() {
        match timeout(Duration::from_secs(5), socket.recv_from(&mut buffer)).await {
            Ok(Ok((size, src))) if size == 2 => {
                let port = u16::from_be_bytes([buffer[0], buffer[1]]);
                return Ok(Some((src.ip().to_string(), port)));
            }
            _ => println!("No response or invalid response from a server."),
        }
    }

    Ok(None)
}

/// Processes a single image by sending it to the server and handling the response.
///
/// # Arguments
/// - `socket`: A reference to the bound `UdpSocket`.
/// - `image_path`: Path to the image to be sent.
/// - `server_addr`: The server's address.
/// - `port`: The port assigned by the server.
/// - `resource_id`: The unique identifier for the image.
/// - `save_dir`: Directory where encrypted and decoded images will be saved.
pub async fn process_image(
    socket: &UdpSocket,
    image_path: &Path,
    server_addr: &str,
    port: u16,
    resource_id: &str,
    save_dir: &Path,
) -> io::Result<()> {
    // Construct paths for saving images
    let encrypted_path = save_dir.join(format!("encrypted_{}.png", resource_id));
    let decoded_path = save_dir.join(format!("decoded_{}.png", resource_id));

    // Clone decoded_path to avoid moving it
    let decoded_path_clone = decoded_path.clone();

    // Send image to the server for encryption
    communication::send_image_over_udp(socket, image_path, server_addr.to_string(), port).await?;
    println!("Image sent to server at {}:{}", server_addr, port);

    // Receive the encrypted image response
    communication::receive_encrypted_image(socket, &encrypted_path).await?;
    println!("Encrypted image received and saved at {:?}", encrypted_path);

    // Decode the encrypted image
    let decode_result = tokio::task::spawn_blocking(move || crate::decode::decode_image(
        encrypted_path.to_str().unwrap(),
        decoded_path_clone.to_str().unwrap(),
    ))
    .await;

    match decode_result {
        Ok(Ok(_)) => println!("Decoded image saved at {:?}", decoded_path),
        Ok(Err(e)) => eprintln!("Failed to decode image: {}", e),
        Err(join_err) => eprintln!("Failed to execute decode_image: {:?}", join_err),
    }

    Ok(())
}

/// Fetches the list of active users and their images from the server.
///
/// # Arguments
/// - `socket`: The UDP socket used for communication.
/// - `config`: The configuration containing server details.
///
/// # Returns
/// A vector of tuples containing peer addresses and their available images.
pub async fn get_active_users(
    socket: &UdpSocket,
    config: &Config,
) -> io::Result<Vec<(String, Vec<String>)>> {
    let (server_addr, assigned_port) = communication::multicast_request(
        socket,
        "active_users",
        &config.server_ips,
        config.request_port,
    )
    .await?;

    let target_server = format!("{}:{}", server_addr, assigned_port);
    socket.send_to(b"request_active_users", &target_server).await?;
    println!("Requested active users from {}", target_server);

    let mut buffer = [0u8; 2048];
    let (size, _) = socket.recv_from(&mut buffer).await?;
    let response = String::from_utf8_lossy(&buffer[..size]);
    let active_users: Vec<(String, Vec<String>)> =
        serde_json::from_str(&response).unwrap_or_default();

    Ok(active_users)
}

/// Sends a request for an image from a peer.
///
/// # Arguments
/// - `socket`: The UDP socket used for communication.
/// - `config`: The configuration containing server details.
/// - `channel`: A channel for managing user-peer interactions.
pub async fn request_image(
    socket: &Arc<UdpSocket>,
    config: &Config,
    channel: &Arc<Mutex<mpsc::Receiver<(String, String)>>>,
) -> io::Result<()> {
    let active_users = get_active_users(socket, config).await?;
    println!("Active Users:");
    for (index, (peer, images)) in active_users.iter().enumerate() {
        println!("{}: {} - {:?}", index + 1, peer, images);
    }

    println!("Enter the number of the user to request from:");
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let user_index = input.trim().parse::<usize>().unwrap() - 1;

    if user_index >= active_users.len() {
        println!("Invalid selection.");
        return Ok(());
    }

    let (peer_addr, images) = &active_users[user_index];
    println!("Available images: {:?}", images);
    println!("Enter the image ID to request:");
    input.clear();
    io::stdin().read_line(&mut input)?;
    let image_id = input.trim();

    println!("Enter the number of views to request:");
    input.clear();
    io::stdin().read_line(&mut input)?;
    let views = input.trim().parse::<u32>().unwrap_or(0);

    // Send P2P request
    p2p::send_image_request(socket, peer_addr, image_id, views).await?;

    println!("Request sent to peer {} for image {}", peer_addr, image_id);

    Ok(())
}

/// Increases the views for an image from an active user.
///
/// # Arguments
/// - `socket`: The UDP socket used for communication.
/// - `config`: The configuration containing server details.
pub async fn increase_image_views(socket: &UdpSocket, config: &Config) -> io::Result<()> {
    let active_users = get_active_users(socket, config).await?;
    println!("Active Users:");
    for (index, (peer, images)) in active_users.iter().enumerate() {
        println!("{}: {} - {:?}", index + 1, peer, images);
    }

    println!("Enter the number of the user to request from:");
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let user_index = input.trim().parse::<usize>().unwrap() - 1;

    if user_index >= active_users.len() {
        println!("Invalid selection.");
        return Ok(());
    }

    let (peer_addr, images) = &active_users[user_index];
    println!("Available images: {:?}", images);
    println!("Enter the image ID to increase views:");
    input.clear();
    io::stdin().read_line(&mut input)?;
    let image_id = input.trim();

    println!("Enter the number of additional views:");
    input.clear();
    io::stdin().read_line(&mut input)?;
    let views = input.trim().parse::<u32>().unwrap_or(0);

    // Send P2P request for increasing views
    p2p::send_image_request(socket, peer_addr, image_id, views).await?;
    println!(
        "Request sent to peer {} to increase views for image {}",
        peer_addr, image_id
    );

    Ok(())
}
