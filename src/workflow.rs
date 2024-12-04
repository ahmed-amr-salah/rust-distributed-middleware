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
use tokio::fs;
use std::collections::HashMap;

use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;



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
    welcome_message: &str,
) -> io::Result<Option<(String, u16)>> {
    let mut buffer = [0u8; 2];

    // Send the resource ID to all servers
    for server_ip in server_ips {
        let server_addr = format!("{}:{}", server_ip, request_port);
 
        socket.send_to(welcome_message.as_bytes(), &server_addr).await?;
    }

    // Wait for a response
    for _ in 0..server_ips.len() {
        match timeout(Duration::from_secs(7), socket.recv_from(&mut buffer)).await {
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

    // Send image to the server for encryption
    communication::send_image_over_udp(socket, image_path, server_addr.to_string(), port).await?;
    println!("Image sent to server at {}:{}", server_addr, port);

    // Receive the encrypted image response
    communication::receive_encrypted_image(socket, &encrypted_path).await?;
    println!("Encrypted image received and saved at {:?}", encrypted_path);

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
    // generate random number
    let seed: [u8; 32] = [0; 32];
    let mut rng = StdRng::from_seed(seed);
    let mut random_num: i32 = rng.gen();

    // Load user_id from the user.json file
    let user_id = match fs::read_to_string("../user.json").await{
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
        "user_id": user_id,
        "random_num": random_num
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
    user_id: String,
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
    let views = views_input.trim().parse::<u16>().unwrap_or(0);
    
    let p2p_socket_response = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);

    // Send P2P request
    println!("Sending request using {}", p2p_socket_response.local_addr().unwrap());
    p2p::send_image_request(&p2p_socket_response, peer_addr, &image_id, views, &user_id).await?;

    println!("Request sent to peer {} for image {}", peer_addr, image_id);

    Ok(())
}


pub async fn request_increase_image_views(socket: &Arc<UdpSocket>, config: &Config) -> io::Result<()> {
    let dir = Path::new("../Peer_Images");
    let json_file_path = dir.join("images_views.json");    
    // Retrieve the peer images I currently have and their views
    let image_views: HashMap<String, u32> = if fs::metadata(&json_file_path).await.is_ok() {
        let json_content = fs::read_to_string(&json_file_path).await?;
        serde_json::from_str(&json_content).unwrap_or_default()        
    } else {
        HashMap::new()
    };

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

    // Check if the entered image_id exists in image_views.json
    if !image_views.contains_key(&image_id) {
        println!("The entered image ID does not exist in your local resources (image_views.json).");
        return Ok(());
    }

    // Check if the image_id is valid for the selected peer
    if !images.contains(&image_id) {
        println!("Invalid image ID.");
        return Ok(());
    }
    
    let user_id: String = match fs::read_to_string("../user.json").await {
        Ok(contents) => {
            let json: serde_json::Value = serde_json::from_str(&contents).unwrap_or_default();
            json.get("user_id")
                .and_then(|id| id.as_str()) // Convert to &str
                .map(|id| id.to_string()) // Convert to String
                .unwrap_or_else(|| "0".to_string()) // Fallback to "0" as a String
        }
        Err(_) => {
            eprintln!("Failed to read user.json or user_id not found. Using default user_id = 0.");
            "0".to_string() // Fallback to "0" as a String
        }
    };
    // Prompt the user to specify additional views
    println!("Enter the number of additional views:");
    input.clear();
    io::stdin().read_line(&mut input)?;
    let views = input.trim().parse::<u16>().unwrap_or(0);

    // Create the JSON payload for the request
    let increase_views_json = json!({
        "type": "increase_views_request",
        "image_id": image_id,
        "views": views,
        "user_id": user_id
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


