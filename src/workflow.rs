use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};
use serde::Deserialize;
use tokio::time::{timeout, Duration};
use crate::communication;
use crate::p2p;
use crate::config::Config;
use serde_json::json;
use std::fs;

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

#[derive(Deserialize, Debug)]
struct ActiveUserEntry {
    client_addr: String,
    client_id: u64,
    image_ids: Vec<String>,
}

pub async fn get_active_users(
    socket: &Arc<UdpSocket>,
    config: &Config,
) -> io::Result<Vec<(String, Vec<String>)>> {
    // Load user_id from the user.json file
    let user_id = match fs::read_to_string("../user.json") {
        Ok(contents) => {
            let json: serde_json::Value = serde_json::from_str(&contents).unwrap_or_default();
            json.get("user_id")
                .and_then(|id| id.as_str()) // Read as a string
                .and_then(|id_str| id_str.parse::<u64>().ok()) // Convert to u64
                .unwrap_or_default() // Default to 0 if parsing fails
        }
        Err(_) => {
            eprintln!("Failed to read user.json or user_id not found. Using default user_id = 0.");
            0 // Default user_id if the file doesn't exist or parsing fails
        }
    };

    // Create the request payload for active users with user_id
    let active_users_request = json!({
        "type": "active_users",
        "user_id": user_id
    });

    // Send the multicast request and await the response
    let (server_addr, response_json) = communication::multicast_request_with_payload(
        socket,
        active_users_request.to_string(),
        &config.server_ips,
        config.request_port,
    )
    .await?;

    println!("Received response from server at {}: {}", server_addr, response_json);

    // Parse the response JSON for active users
    let active_users: Vec<ActiveUserEntry> = response_json
        .get("data") // Access the "data" key in the response
        .and_then(|data| serde_json::from_value(data.clone()).ok()) // Deserialize into Vec<ActiveUserEntry>
        .unwrap_or_default();

    // Convert to Vec<(String, Vec<String>)> format
    let result: Vec<(String, Vec<String>)> = active_users
        .into_iter()
        .map(|entry| (entry.client_addr, entry.image_ids))
        .collect();

    Ok(result)
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
    let user_index = match input.trim().parse::<usize>() {
        Ok(index) if index > 0 && index <= active_users.len() => index - 1,
        _ => {
            println!("Invalid selection. Please enter a valid number.");
            return Ok(()); // Return to the main menu
        }
    };

    let (peer_addr, images) = &active_users[user_index];

    println!("Available images: {:?}", images);

    // Requesting the image
    println!("Enter the image ID to request:");
    let mut image_input = String::new(); // Use a new `String` for this part to avoid conflicts
    io::stdin().read_line(&mut image_input)?;
    let image_id = image_input.trim().to_string(); // Clone the trimmed value

    // Requesting views
    println!("Enter the number of views to request:");
    let mut views_input = String::new(); // Another new `String` to handle this input
    io::stdin().read_line(&mut views_input)?;
    let views = views_input.trim().parse::<u32>().unwrap_or(0);

    // Send P2P request
    p2p::send_image_request(socket, peer_addr, &image_id, views).await?;

    println!("Request sent to peer {} for image {}", peer_addr, image_id);

    Ok(())
}




pub async fn increase_image_views(socket: &Arc<UdpSocket>, config: &Config) -> io::Result<()> {
    // Retrieve active users from the server
    let active_users = get_active_users(socket, config).await?;
    println!("Active Users:");

    // Display active users and their images
    for (index, (peer, images)) in active_users.iter().enumerate() {
        println!("{}: {} - {:?}", index + 1, peer, images);
    }

    // Prompt the user to select a peer
    println!("Enter the number of the user to request from:");
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let user_index = input.trim().parse::<usize>().unwrap_or(0) - 1;

    if user_index >= active_users.len() {
        println!("Invalid selection.");
        return Ok(());
    }

    // Extract the selected peer and their images
    let (peer_addr, images) = &active_users[user_index];
    println!("Available images from {}: {:?}", peer_addr, images);

    // Prompt the user to select an image
    println!("Enter the image ID to increase views:");
    input.clear();
    io::stdin().read_line(&mut input)?;
    let image_id = input.trim().to_string();

    if !images.contains(&image_id) {
        println!("Invalid image ID.");
        return Ok(());
    }

    // Prompt the user to specify additional views
    println!("Enter the number of additional views:");
    input.clear();
    io::stdin().read_line(&mut input)?;
    let views = input.trim().parse::<u32>().unwrap_or(0);

    // Create the JSON payload for the request
    let increase_views_json = json!({
        "type": "increase_views",
        "image_id": image_id,
        "views": views
    });

    // Send the P2P request to the selected peer
    socket
        .send_to(increase_views_json.to_string().as_bytes(), peer_addr)
        .await?;
    println!(
        "Request sent to {} to increase views for image {} by {} views.",
        peer_addr, image_id, views
    );

    Ok(())
}


