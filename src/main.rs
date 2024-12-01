use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use serde_json::json;
use uuid::Uuid;
use std::fs;
use std::collections::VecDeque;

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
    let socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?); // Wrap in Arc
    let request_queue = Arc::new(Mutex::new(VecDeque::new()));

    // P2P communication setup
    let (p2p_tx, mut p2p_rx) = mpsc::channel(100);
    let p2p_socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);

    println!("P2P Socket {}", p2p_socket.local_addr()?);
    println!("Socket {}", socket.local_addr()?);

    let request_queue_clone = Arc::clone(&request_queue);
    let p2p_socket_clone = Arc::clone(&p2p_socket);

    tokio::spawn({
        async move {
            let mut buffer = [0u8; 1024];
            loop {
                if let Ok((size, src)) = p2p_socket_clone.recv_from(&mut buffer).await {
                    let received_data = String::from_utf8_lossy(&buffer[..size]).to_string();
                    println!("[P2P Listener] Received data: {} from {}", received_data, src);
                    // Parse the payload to JSON
                    let response_json = match serde_json::from_str::<serde_json::Value>(&received_data) {
                        Ok(json) => json,
                        Err(_) => {
                            // Log and ignore non-JSON payloads
                            eprintln!("[P2P Listener] Ignoring non-JSON payload: {}", received_data);
                            continue;
                        }
                    };
                    if let Ok(response_json) = serde_json::from_str::<serde_json::Value>(&received_data) {
                        if let Some(response_type) = response_json.get("type").and_then(|t| t.as_str()) {
                            match response_type {
                            // *** Automatically handle "increase_approved" ***
                            "increase_approved" => {
                                if let Some(image_id) = response_json.get("image_id").and_then(|id| id.as_str()) {
                                    if let Some(additional_views) = response_json.get("views").and_then(|v| v.as_u64()) {
                                        println!(
                                            "[P2P Listener] Automatically handling 'increase_approved' for image '{}' with {} additional views.",
                                            image_id, additional_views
                                        );
                                        p2p::handle_increase_views_response(image_id, additional_views as u32, true).await;
                                    } else {
                                        eprintln!("[P2P Listener] Missing or invalid 'views' in 'increase_approved' payload.");
                                    }
                                } else {
                                    eprintln!("[P2P Listener] Missing 'image_id' in 'increase_approved' payload.");
                                }
                            }
    
                            // *** Automatically handle "increase_rejected" ***
                            "increase_rejected" => {
                                if let Some(image_id) = response_json.get("image_id").and_then(|id| id.as_str()) {
                                    if let Some(additional_views) = response_json.get("views").and_then(|v| v.as_u64()) {
                                        println!(
                                            "[P2P Listener] Automatically handling 'increase_approved' for image '{}' with {} additional views.",
                                            image_id, additional_views
                                        );
                                        p2p::handle_increase_views_response(image_id, additional_views as u32, false).await;
                                    } else {
                                        eprintln!("[P2P Listener] Missing or invalid 'views' in 'increase_approved' payload.");
                                    }
                                } else {
                                    eprintln!("[P2P Listener] Missing 'image_id' in 'increase_approved' payload.");
                                }
                            }
                            "rejection_ack" => {
                                if let Some(image_id) = response_json.get("image_id").and_then(|id| id.as_str()) {
                                    if let Some(status) = response_json.get("status").and_then(|s| s.as_str()) {
                                        println!(
                                            "[P2P Listener] Received rejection acknowledgment for image '{}' with status '{}'.",
                                            image_id, status
                                        );
                                        // Additional logic for handling acknowledgment can be added here
                                    } else {
                                        eprintln!("[P2P Listener] Missing 'status' in 'rejection_ack' payload.");
                                    }
                                } else {
                                    eprintln!("[P2P Listener] Missing 'image_id' in 'rejection_ack' payload.");
                                }
                            }
                            // *** Add other payloads to the queue ***
                            _ => {
                                println!(
                                    "[P2P Listener] Adding payload of type '{}' from {} to the queue.",
                                    response_type, src
                                );
                                let mut queue = request_queue_clone.lock().await;
                                queue.push_back((src, received_data)); // Add the payload to the queue
                            }
                        }
                    } else {
                        eprintln!("[P2P Listener] Malformed payload: Missing 'type' field.");
                    }
                } else {
                    eprintln!("[P2P Listener] Failed to parse received data as JSON: {}", received_data);
                }
            }
        }
    }
});
    
    // Spawn P2P listener task
    // let p2p_socket_clone = Arc::clone(&p2p_socket);
    // tokio::spawn(async move {
    //     let mut buffer = [0u8; 1024];
    //     loop {
    //          println!("Hellooo");
    //         if let Ok((size, src)) = p2p_socket_clone.recv_from(&mut buffer).await {
    //             let request = String::from_utf8_lossy(&buffer[..size]).to_string();
    //             println!("[P2P Listener] Received request: {} from {}", request, src);

    //             if let Ok(request_json) = serde_json::from_str::<serde_json::Value>(&request) {
    //                 if let Some(request_type) = request_json.get("type").and_then(|t| t.as_str()) {
    //                     match request_type {
    //                         "image_request" => {
    //                             if let Some(image_id) = request_json.get("image_id").and_then(|id| id.as_str()) {
    //                                 if let Some(requested_views) = request_json.get("views").and_then(|v| v.as_u64()) {
    //                                     let views = requested_views as u16;
    //                                     println!("received request for image from {}", src.to_string());
    //                                     if let Err(e) = p2p::respond_to_request(
    //                                         &p2p_socket_clone,
    //                                         image_id,
    //                                         views,
    //                                         &src.to_string(),
    //                                     )
    //                                     .await
    //                                     {
    //                                         eprintln!("[P2P Listener] Error responding to request: {}", e);
    //                                     }
    //                                 } else {
    //                                     eprintln!("[P2P Listener] Missing or invalid 'views' field in request");
    //                                 }
    //                             } else {
    //                                 eprintln!("[P2P Listener] Missing 'image_id' in request");
    //                             }
    //                         }
                            // "image_response" => {
                            //     if let Some(image_id) = request_json.get("image_id").and_then(|id| id.as_str()) {
                            //         if let Some(encoded_data) = request_json.get("data").and_then(|d| d.as_str()) {
                            //             match base64::decode(encoded_data) {
                            //                 Ok(image_data) => {
                            //                     if let Err(e) = p2p::store_received_image(image_id, &image_data).await {
                            //                         eprintln!("[P2P Listener] Failed to store image: {}", e);
                            //                     }
                            //                 }
                            //                 Err(e) => eprintln!("[P2P Listener] Failed to decode image data: {}", e),
                            //             }
                            //         } else {
                            //             eprintln!("[P2P Listener] Missing or invalid 'data' field in response");
                            //         }
                            //     } else {
                            //         eprintln!("[P2P Listener] Missing 'image_id' in response");
                            //     }
                            // }
    //                         _ => eprintln!("[P2P Listener] Unknown request type: {}", request_type),
    //                     }
    //                 } else {
    //                     eprintln!("[P2P Listener] Malformed request: Missing 'type'");
    //                 }
    //             } else {
    //                 eprintln!("[P2P Listener] Failed to parse request as JSON");
    //             }
    //         }
    //     }
    // });

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
                // Create the registration JSON payload
                let register_json = json!({
                    "type": "register"
                });

                // Multicast registration request with the JSON payload
                let (server_addr, response_json) = communication::multicast_request_with_payload(
                    &socket,
                    register_json.to_string(),
                    &config.server_ips,
                    config.request_port,
                )
                .await?;

                println!("Received response from server at {}: {}", server_addr, response_json);

                // Handle registration response
                if let Some(user_id) = response_json.get("user_id").and_then(|id| id.as_u64()) {
                    let user_id_str = user_id.to_string(); // Convert u64 to String for saving
                    authentication::save_user_id(&user_id_str, Path::new("user.json"))?;
                    println!("Registration successful! User ID: {}", user_id);
                } else {
                    eprintln!("Error: Failed to retrieve user_id from server response.");
                }

                println!("Returning to the main menu...");
            }
            "2" => {
                print!("Enter your user ID: ");
                io::stdout().flush()?;
                let mut user_id_input = String::new();
                io::stdin().read_line(&mut user_id_input)?;
                let user_id_input = user_id_input.trim();

                // Parse the user ID as u64
                let user_id: u64 = match user_id_input.parse() {
                    Ok(id) => id,
                    Err(_) => {
                        eprintln!("Invalid user ID. Please enter a numeric value.");
                        continue; // Return to the main menu
                    }
                };
                let local_addr = p2p_socket.local_addr()?;
                // Format the address as ip_address:port
                let p2p_formatted_socket = local_addr.to_string();
                // Create the authentication JSON payload
                let auth_json = json!({
                    "type": "sign_in",
                    "user_id": user_id,
                    "p2p_socket": p2p_formatted_socket
                });

                // Multicast sign-in request
                let (server_addr, response_json) = communication::multicast_request_with_payload(
                    &socket,
                    auth_json.to_string(),
                    &config.server_ips,
                    config.request_port,
                )
                .await?;

                println!("Received response from server at {}: {}", server_addr, response_json);

                // Check for success in JSON response
                if response_json.get("status") == Some(&serde_json::Value::String("success".to_string())) {
                    println!("Sign-in successful!");

                    // Main menu after successful sign-in
                    let peer_channel = Arc::new(Mutex::new(p2p_rx));
                    loop {
                        println!("\nMain Menu:");
                        println!("1. Encode Image");
                        println!("2. Request List of Active Users and Their Images");
                        println!("3. Request Image From Active User");
                        println!("4. Increase Views of Image From Active User");
                        println!("5. View Client Requests");
                        println!("6. Shutdown");
                        print!("Enter your choice: ");
                        io::stdout().flush()?;
                        let mut menu_choice = String::new();
                        io::stdin().read_line(&mut menu_choice)?;
                        let menu_choice = menu_choice.trim();

                        match menu_choice {
                            "1" => {
                                // User input: image path and resource ID
                                let mut input = String::new();
                                print!("Enter the path to the image: ");
                                io::stdout().flush()?;
                                io::stdin().read_line(&mut input)?;
                                let image_path = Path::new(input.trim());

                                // Validate the image path
                                if !image_path.exists() {
                                    eprintln!("The specified image path does not exist.");
                                    continue;
                                }
                                if !image_path.is_file() {
                                    eprintln!("The specified path is not a file.");
                                    continue;
                                }

                                // Generate a random resource ID
                                // let resource_id = Uuid::new_v4().to_string();
                                // println!("Generated unique resource ID: {}", resource_id);

                                // Extract the image file name (without extension) as the resource ID
                                let resource_name = image_path.file_stem()
                                    .and_then(|os_str| os_str.to_str()) // Convert OsStr to str
                                    .unwrap_or("unknown") // Fallback if file_stem is invalid
                                    .to_string();


                                let user_id: String = match fs::read_to_string("../user.json") {
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
                                    
                                
                                // Create the concatenated resource ID
                                let resource_id = format!("client{}-{}", user_id, resource_name);
                                // create json as string that has used_id and resource_id
                                let welcome_message = format!("{},{}", user_id, resource_id);

                                println!("Concatenated Resource ID: {}", resource_id);

                                // Find the server handling the request
                                if let Some((server_addr, port)) = workflow::find_server_for_resource(
                                    &socket,
                                    &config.server_ips,
                                    config.request_port,
                                    &welcome_message,
                                )
                                .await?
                                {
                                    println!(
                                        "Server {}:{} will handle the request for resource ID {}",
                                        server_addr, port, resource_id
                                    );

                                    // Process the single image
                                    workflow::process_image(
                                        &socket,
                                        image_path,
                                        &server_addr,
                                        port,
                                        &resource_id,
                                        &config.save_dir,
                                    )
                                    .await?;
                                } else {
                                    eprintln!("No server responded to the resource ID. Exiting.");
                                }
                            }
                            "2" => {
                                let active_users = workflow::get_active_users(&socket, &config).await?;
                                println!("Active Users and Their Images:");
                                if active_users.is_empty() {
                                    println!("No active users found.");
                                } else {
                                    for (peer, images) in active_users.iter() {
                                        println!("Peer: {}", peer);
                                        println!("Images: {:?}", images);
                                    }
                                }
                            }
                            "3" => {
                                workflow::request_image(&p2p_socket, &config, &peer_channel).await?;
                            }
                            "4" => {
                                workflow::request_increase_image_views(&p2p_socket, &config).await?;
                            }
                            "5" => {
                                // REQUESTS QUEUE HANDLING LOGIC HERE
                                let mut queue = request_queue.lock().await;
                                if queue.is_empty() {
                                    println!("Request queue is empty.");
                                } else {
                                    println!("Request Queue:");
                                    for (index, (src, request)) in queue.iter().enumerate() {
                                        println!("{}. [{}] {}", index + 1, src, request);
                                    }

                                    println!("Enter the number of the request to handle, or '0' to go back:");
                                    let mut input = String::new();
                                    io::stdin().read_line(&mut input)?;
                                    let input = input.trim();

                                    match input.parse::<usize>() {
                                        Ok(0) => continue, // Go back to the main menu
                                        Ok(index) if index > 0 && index <= queue.len() => {
                                            let (src, request) = queue.remove(index - 1).unwrap();
                                            println!("Selected request: [{}] {}", src, request);

                                            println!("Options:");
                                            println!("1. Approve");
                                            println!("2. Reject");
                                            println!("3. Go back");
                                            print!("Enter your choice: ");
                                            io::stdout().flush()?;

                                            let mut action = String::new();
                                            io::stdin().read_line(&mut action)?;
                                            let action = action.trim();

                                            match action {
                                                "1" => {
                                                    println!("Approved request from {}: {}", src, request);
                                                    // Parse the request JSON
                                                    if let Ok(request_json) = serde_json::from_str::<serde_json::Value>(&request) {
                                                        if let Some(request_type) = request_json.get("type").and_then(|t| t.as_str()) {
                                                            match request_type {
                                                                "image_request" => {
                                                                    if let Some(image_id) = request_json.get("image_id").and_then(|id| id.as_str()) {
                                                                        if let Some(requested_views) = request_json.get("views").and_then(|v| v.as_u64()) {
                                                                            let views = requested_views as u16;
                                                                            println!("Processing 'image_request' for image_id: {}, views: {}", image_id, views);

                                                                            // Call the appropriate function to handle the image request
                                                                            if let Err(e) = p2p::respond_to_request(
                                                                                &p2p_socket,
                                                                                image_id,
                                                                                views,
                                                                                &src.to_string(),
                                                                                true
                                                                            )
                                                                            .await
                                                                            {
                                                                                eprintln!("[Request Handler] Error responding to request: {}", e);
                                                                            } else {
                                                                                println!("[Request Handler] Successfully handled 'image_request'.");
                                                                            }
                                                                        } else {
                                                                            eprintln!("[Request Handler] Missing or invalid 'views' field in 'image_request'");
                                                                        }
                                                                    } else {
                                                                        eprintln!("[Request Handler] Missing 'image_id' field in 'image_request'");
                                                                    }
                                                                }
                                                                "increase_views_request" => {
                                                                    if let Some(image_id) = request_json.get("image_id").and_then(|id| id.as_str()) {
                                                                        if let Some(requested_views) = request_json.get("views").and_then(|v| v.as_u64()) {
                                                                            let views = requested_views as u16;
                                                                            println!("Processing 'increase_views_request' for image_id: {}, views: {}", image_id, views);

                                                                            // Call the appropriate function to handle the image request
                                                                            if let Err(e) = p2p::respond_to_increase_views(
                                                                                &p2p_socket,
                                                                                image_id,
                                                                                views,
                                                                                &src.to_string(),
                                                                                true
                                                                            )
                                                                            .await
                                                                            {
                                                                                eprintln!("[Request Handler] Error responding to request: {}", e);
                                                                            } else {
                                                                                println!("[Request Handler] Successfully handled 'increase_views_request'.");
                                                                            }
                                                                        } else {
                                                                            eprintln!("[Request Handler] Missing or invalid 'views' field in 'increase_views_request'");
                                                                        }
                                                                    } else {
                                                                        eprintln!("[Request Handler] Missing 'image_id' field in 'increase_views_request'");
                                                                    }
                                                                }
                                                                // Add more request types as needed
                                                                "another_request_type" => {
                                                                    println!("Processing 'another_request_type': {:?}", request_json);
                                                                    // Add logic for handling 'another_request_type'
                                                                }
                                                                _ => eprintln!("[Request Handler] Unknown request type: {}", request_type),
                                                            }
                                                        } else {
                                                            eprintln!("[Request Handler] Malformed request: Missing 'type'");
                                                        }
                                                    } else {
                                                        eprintln!("[Request Handler] Failed to parse request as JSON");
                                                    }
                                                }
                                                "2" => {
                                                    println!("Rejected request from {}: {}", src, request);
                                                    // Parse the request JSON
                                                    if let Ok(request_json) = serde_json::from_str::<serde_json::Value>(&request) {
                                                        if let Some(request_type) = request_json.get("type").and_then(|t| t.as_str()) {
                                                            match request_type {
                                                                "image_request" => {
                                                                    if let Some(image_id) = request_json.get("image_id").and_then(|id| id.as_str()) {
                                                                        if let Some(requested_views) = request_json.get("views").and_then(|v| v.as_u64()) {
                                                                            let views = requested_views as u16;
                                                                            println!("Processing 'image_request' for image_id: {}, views: {}", image_id, views);

                                                                            // Call the appropriate function to handle the image request
                                                                            if let Err(e) = p2p::respond_to_request(
                                                                                &p2p_socket,
                                                                                image_id,
                                                                                views,
                                                                                &src.to_string(),
                                                                                false
                                                                            )
                                                                            .await
                                                                            {
                                                                                eprintln!("[Request Handler] Error responding to request: {}", e);
                                                                            } else {
                                                                                println!("[Request Handler] Successfully handled 'image_request'.");
                                                                            }
                                                                        } else {
                                                                            eprintln!("[Request Handler] Missing or invalid 'views' field in 'image_request'");
                                                                        }
                                                                    } else {
                                                                        eprintln!("[Request Handler] Missing 'image_id' field in 'image_request'");
                                                                    }
                                                                }
                                                                "increase_views_request" => {
                                                                    if let Some(image_id) = request_json.get("image_id").and_then(|id| id.as_str()) {
                                                                        if let Some(requested_views) = request_json.get("views").and_then(|v| v.as_u64()) {
                                                                            let views = requested_views as u16;
                                                                            println!("Processing 'increase_views_request' for image_id: {}, views: {}", image_id, views);

                                                                            // Call the appropriate function to handle the image request
                                                                            if let Err(e) = p2p::respond_to_increase_views(
                                                                                &p2p_socket,
                                                                                image_id,
                                                                                views,
                                                                                &src.to_string(),
                                                                                false
                                                                            )
                                                                            .await
                                                                            {
                                                                                eprintln!("[Request Handler] Error responding to request: {}", e);
                                                                            } else {
                                                                                println!("[Request Handler] Successfully handled 'increase_views_request'.");
                                                                            }
                                                                        } else {
                                                                            eprintln!("[Request Handler] Missing or invalid 'views' field in 'increase_views_request'");
                                                                        }
                                                                    } else {
                                                                        eprintln!("[Request Handler] Missing 'image_id' field in 'increase_views_request'");
                                                                    }
                                                                }
                                                                // Add more request types as needed
                                                                "another_request_type" => {
                                                                    println!("Processing 'another_request_type': {:?}", request_json);
                                                                    // Add logic for handling 'another_request_type'
                                                                }
                                                                _ => eprintln!("[Request Handler] Unknown request type: {}", request_type),
                                                            }
                                                        } else {
                                                            eprintln!("[Request Handler] Malformed request: Missing 'type'");
                                                        }
                                                    } else {
                                                        eprintln!("[Request Handler] Failed to parse request as JSON");
                                                    }
                                                },
                                                "3" => {
                                                    println!("Returning to queue...");
                                                    queue.push_front((src, request)); // Return the request to the front of the queue
                                                }
                                                _ => println!("Invalid choice. Returning to queue..."),
                                            }
                                        }
                                        _ => println!("Invalid selection. Please enter a valid number."),
                                    }
                                }
                            } 
                            "6" => {
                                // Multicast shutdown request
                                let shutdown_json = json!({
                                    "type": "shutdown",
                                    "user_id": user_id
                                });

                                let (server_addr, shutdown_response) =
                                    communication::multicast_request_with_payload(
                                        &socket,
                                        shutdown_json.to_string(),
                                        &config.server_ips,
                                        config.request_port,
                                    )
                                    .await?;

                                println!("Sent shutdown request to {}", server_addr);
                                println!("Shutdown response: {}", shutdown_response);
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
