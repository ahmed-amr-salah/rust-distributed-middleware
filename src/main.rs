use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};
use serde_json::json;
use uuid::Uuid;

mod authentication;
mod communication;
mod decode;
mod encode;
mod config;
mod workflow;
mod p2p;

#[tokio::main]
async fn main() -> io::Result<()> {
    // Load environment variables and configuration
    let config = config::load_config();
    let socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);

    // Set up channel for peer-to-peer communication
    let (p2p_tx, mut p2p_rx) = mpsc::channel::<(String, String)>(100);
    let p2p_socket = Arc::clone(&socket);

    // Spawn P2P listener thread
    tokio::spawn(async move {
        let mut buffer = [0u8; 1024];
        loop {
            if let Ok((size, src)) = p2p_socket.recv_from(&mut buffer).await {
                let request = String::from_utf8_lossy(&buffer[..size]).to_string();
                println!("[P2P Listener] Received request: {} from {}", request, src);

                // Forward request to the main thread via the channel
                if let Err(e) = p2p_tx.send((src.to_string(), request)).await {
                    eprintln!("[P2P Listener] Failed to send request to main thread: {}", e);
                }
            }
        }
    });

    // Main menu loop
    loop {
        // Authentication menu
        println!("Welcome! Please choose an option:");
        println!("1. Register");
        println!("2. Sign In");
        print!("Enter your choice: ");
        io::stdout().flush()?;

        let mut choice = String::new();
        io::stdin().read_line(&mut choice)?;
        let choice = choice.trim();

        match choice {
            "1" => {
                // Multicast registration request
                let (server_addr, assigned_port) = communication::multicast_request(
                    &socket,
                    "register",
                    &config.server_ips,
                    config.request_port,
                )
                .await?;

                let target_server = format!("{}:{}", server_addr.ip(), assigned_port);

                // Create and send registration JSON
                let auth_json = authentication::create_auth_json(None);
                socket.send_to(auth_json.to_string().as_bytes(), &target_server).await?;
                println!("Sent registration request to {}", target_server);

                // Receive server response
                let mut response_buffer = [0u8; 1024];
                let (size, _) = socket.recv_from(&mut response_buffer).await?;
                let response = String::from_utf8_lossy(&response_buffer[..size]);
                println!("Registration response: {}", response);

                if response.contains("user_id") {
                    let parsed_response: serde_json::Value = serde_json::from_str(&response)?;
                    if let Some(user_id) = parsed_response["user_id"].as_str() {
                        authentication::save_user_id(user_id, Path::new("user.json"))?;
                    } else {
                        eprintln!("Error: Failed to retrieve user_id from response.");
                    }
                }
                println!("Registration successful! Returning to the main menu...");
            }
            "2" => {
                // Multicast sign-in request
                let (server_addr, assigned_port) = communication::multicast_request(
                    &socket,
                    "auth",
                    &config.server_ips,
                    config.request_port,
                )
                .await?;

                let target_server = format!("{}:{}", server_addr.ip(), assigned_port);

                // Sign In
                print!("Enter your user ID: ");
                io::stdout().flush()?;
                let mut user_id = String::new();
                io::stdin().read_line(&mut user_id)?;
                let user_id = user_id.trim();

                let auth_json = authentication::create_auth_json(Some(user_id));
                socket.send_to(auth_json.to_string().as_bytes(), &target_server).await?;
                println!("Sent sign-in request to {}", target_server);

                // Handle server response
                let mut response_buffer = [0u8; 1024];
                let (size, _) = socket.recv_from(&mut response_buffer).await?;
                let response = String::from_utf8_lossy(&response_buffer[..size]);
                println!("Sign-in response: {}", response);

                if response.contains("success") {
                    println!("Sign-in successful!");

                    // Main menu after successful sign-in
                    let peer_channel = Arc::new(Mutex::new(p2p_rx));
                    loop {
                        println!("\nMain Menu:");
                        println!("1. Encode Image");
                        println!("2. Request List of Active Users and Their Images");
                        println!("3. Request Image From Active User");
                        println!("4. Increase Views of Image From Active User");
                        println!("5. Shutdown");
                        print!("Enter your choice: ");
                        io::stdout().flush()?;
                        let mut menu_choice = String::new();
                        io::stdin().read_line(&mut menu_choice)?;
                        let menu_choice = menu_choice.trim();

                        match menu_choice {
                            "1" => {
                                //workflow::encode_image(&socket, &config).await?;
                            }
                            "2" => {
                                let active_users = workflow::get_active_users(&socket, &config).await?;
                                println!("Active Users and Their Images:");
                                for (peer, images) in active_users.iter() {
                                    println!("{}: {:?}", peer, images);
                                }
                            }
                            "3" => {
                                workflow::request_image(&socket, &config, &peer_channel).await?;
                            }
                            "4" => {
                                workflow::increase_image_views(&socket, &config).await?;
                            }
                            "5" => {
                                // Shutdown request
                                let (server_addr, assigned_port) =
                                    communication::multicast_request(
                                        &socket,
                                        "shutdown",
                                        &config.server_ips,
                                        config.request_port,
                                    )
                                    .await?;

                                let target_server =
                                    format!("{}:{}", server_addr.ip(), assigned_port);

                                let shutdown_json = json!({
                                    "type": "shutdown",
                                    "user_id": user_id
                                });
                                socket
                                    .send_to(shutdown_json.to_string().as_bytes(), &target_server)
                                    .await?;
                                println!("Sent shutdown request to {}", target_server);

                                let mut response_buffer = [0u8; 1024];
                                let (size, _) = socket.recv_from(&mut response_buffer).await?;
                                let response =
                                    String::from_utf8_lossy(&response_buffer[..size]);
                                println!("Shutdown response: {}", response);
                                break;
                            }
                            _ => {
                                println!("Invalid choice. Please try again.");
                            }
                        }
                    }
                    break; // Exit the loop after shutdown
                } else {
                    println!("Sign-in failed. Returning to the main menu...");
                }
            }
            _ => {
                println!("Invalid choice. Please try again.");
            }
        }
    }

    Ok(())
}
