use std::io::{self, Write,Read};
use std::path::Path;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use serde_json::{json, Value, Map};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::fs;
use std::fs::File;
use std::collections::VecDeque;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use image::DynamicImage;
use std::process::Command;
mod open_image; // Add this at the top of main.rs
use open_image::open_image_with_default_viewer;

mod authentication;
mod communication;
mod decode;
mod encode;
mod config;
mod workflow;
mod p2p;



#[tokio::main]
#[show_image::main]
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
                                                "[P2P Listener] Handling 'increase_approved' for image '{}' with {} additional views.",
                                                image_id, additional_views
                                            );
                                            p2p::handle_increase_views_response(image_id, additional_views as u32, true).await;
                                            
                                            // Send acknowledgment
                                            let ack_message = json!({
                                                "type": "increase_approved_ack",
                                                "status": "received",
                                                "image_id": image_id
                                            }).to_string();
                                            if let Err(e) = p2p_socket_clone.send_to(ack_message.as_bytes(), src).await {
                                                eprintln!("[P2P Listener] Failed to send acknowledgment: {}", e);
                                            } else {
                                                println!("[P2P Listener] Sent acknowledgment for 'increase_approved' to {}", src);
                                            }
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
                                                "[P2P Listener] Handling 'increase_rejected' for image '{}' with {} additional views.",
                                                image_id, additional_views
                                            );
                                            p2p::handle_increase_views_response(image_id, additional_views as u32, false).await;
        
                                            // Send acknowledgment
                                            let ack_message = json!({
                                                "type": "increase_rejected_ack",
                                                "status": "received",
                                                "image_id": image_id
                                            }).to_string();
                                            if let Err(e) = p2p_socket_clone.send_to(ack_message.as_bytes(), src).await {
                                                eprintln!("[P2P Listener] Failed to send acknowledgment: {}", e);
                                            } else {
                                                println!("[P2P Listener] Sent acknowledgment for 'increase_rejected' to {}", src);
                                            }
                                        } else {
                                            eprintln!("[P2P Listener] Missing or invalid 'views' in 'increase_rejected' payload.");
                                        }
                                    } else {
                                        eprintln!("[P2P Listener] Missing 'image_id' in 'increase_rejected' payload.");
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
                // Generate a random seed for the RNG
                let seed: [u8; 32] = [0; 32];
                let mut rng = StdRng::from_seed(seed);
                let random_num: i32 = rng.gen();
                // Create the registration JSON payload
                let register_json = json!({
                    "type": "register",
                    "randam_number": random_num,
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
                    authentication::save_user_id(&user_id_str, Path::new("../user.json"))?;
                    println!("Registration successful! User ID: {}", user_id);
                } else {
                    eprintln!("Error: Failed to retrieve user_id from server response.");
                }

                println!("Returning to the main menu...");
            }
            "2" => {
                // Generate a random seed for the RNG
                let seed: [u8; 32] = [0; 32];
                let mut rng = StdRng::from_seed(seed);
                let random_num: i32 = rng.gen();
                // start the logic 
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

                // Save the entered user_id to user.json
                let user_id_str = user_id.to_string(); // Convert to string for saving
                let user_data = json!({ "user_id": user_id_str });

                if let Err(e) = std::fs::write("../user.json", user_data.to_string()) {
                    eprintln!("Failed to overwrite user.json with new user ID: {}", e);
                } else {
                    println!("User ID saved successfully to user.json.");
                }
                
                let local_addr = p2p_socket.local_addr()?;
                // Format the address as ip_address:port
                let p2p_formatted_socket = local_addr.to_string();
                // Create the authentication JSON payload
                let auth_json = json!({
                    "type": "sign_in",
                    "user_id": user_id,
                    "p2p_socket": p2p_formatted_socket,
                    "randam_number": random_num,
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

                    // Check if "resources" in the response is non-empty and update views
                    if let Some(resources) = response_json.get("resources").and_then(|r| r.as_array()) {
                        if resources.is_empty() {
                            eprintln!("No resources available in response.");
                        } else {
                            for resource in resources {
                                if let (Some(image_id), Some(views)) = (
                                    resource.get("image_id").and_then(|id| id.as_str()),
                                    resource.get("views").and_then(|v| v.as_u64()),
                                ) {
                                    println!("Updating views for image '{}': {} views", image_id, views);
                                    if let Err(e) = p2p::handle_increase_views_response(image_id, views as u32, true).await {
                                        eprintln!("Failed to update views for image '{}': {}", image_id, e);
                                    }
                                } else {
                                    eprintln!("Malformed resource entry: {:?}", resource);
                                }
                            }
                        }
                    } else {
                        eprintln!("Missing or invalid 'resources' field in response.");
                    }

                    // Main menu after successful sign-in
                    let peer_channel = Arc::new(Mutex::new(p2p_rx));
                    loop {
                        println!("\nMain Menu:");
                        println!("1. Encode Image");
                        println!("2. Request List of Active Users and Their Images");
                        println!("3. Request Image From Active User");
                        println!("4. Increase Views of Image From Active User");
                        println!("5. View Client Requests");
                        println!("6. View Peer Images");
                        println!("7. Shutdown");
                        print!("Enter your choice: ");
                        io::stdout().flush()?;
                        let mut menu_choice = String::new();
                        io::stdin().read_line(&mut menu_choice)?;
                        let menu_choice = menu_choice.trim();

                        match menu_choice {
                            "1" => {
                                // generate random seed for the RNG
                                let seed: [u8; 32] = [0; 32];
                                let mut rng = StdRng::from_seed(seed);
                                let mut random_num: i32 = rng.gen();

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
                                //craft welcome message to to has the  user_id, resource_id and counter
                                let welcome_message = format!("{},{},{}", user_id, resource_id,random_num); 

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
                                // Generate a random seed for the RNG
                                let seed: [u8; 32] = [0; 32];
                                let mut rng = StdRng::from_seed(seed);
                                let random_num: i32 = rng.gen();
                                 
                                //Viewing Images we have access to
                                //PseudoCode
                                // retrieve images_views.json file and display to user and wait for 
                                // user to choose image_id
                                //retrive {image_id}.png from received_images and show it in a pop up widow
                                //decrement the value corresponding to the image_id to image_views.json
                                //if the value is zero we will delete {image_id}.png
                                //and delete the entry for this image id in image_views.json
                                //and print that we ran out of viewing rights
                                
                                let json_image_views_path = "../Peer_Images/images_views.json";
                                let images_folder = "../Peer_Images";
                                
                                let mut file = File::open(json_image_views_path).expect("Failed to open images_views.json");
                                let mut contents = String::new();
                                file.read_to_string(&mut contents).expect("Failed to read images_views.json");

                                // Parse JSON
                                let mut image_views: Map<String, Value> = serde_json::from_str(&contents)
                                    .expect("Invalid JSON format in images_views.json");

                                // Step 2: Display available image IDs and view counts
                                println!("Available Images:");
                                for (image_id, count) in &image_views{
                                    println!("{} -> Remaining Views: {}", image_id, count.as_u64().unwrap_or(0));
                                }

                                // Step 3: Prompt the user to select an image ID
                                println!("Enter the Image ID you want to view:");
                                let mut selected_image_id = String::new();
                                io::stdin().read_line(&mut selected_image_id).expect("Failed to read input");
                                let selected_image_id = selected_image_id.trim();

                                // Step 4: Validate the selection
                                if let Some(remaining_views) = image_views.get_mut(selected_image_id) {
                                    let remaining_views = remaining_views.as_u64().expect("Invalid view count");
                                    
                                        // Step 5: Construct the path to the image
                                        let image_path = format!("{}/.{}.png", images_folder, selected_image_id);
                                    if remaining_views > 0 {
                                        // Check if the image exists
                                        if !Path::new(&image_path).exists() {
                                            println!("Image file not found: {}", image_path);
                                            return Ok(());
                                        }

                                        // Step 6: Open the image using the system's default viewer
                                        println!("Image Path is {}", image_path);
                                        if let Err(e) = open_image_with_default_viewer(&image_path).await {
                                            eprintln!("Error: {}", e);
                                        }
                                        // Step 7: Decrement the view count
                                    
                                        *image_views.get_mut(selected_image_id).unwrap() = Value::from(remaining_views - 1);
                                        println!(
                                            "Remaining views for image {}: {}",
                                            selected_image_id,
                                            remaining_views - 1
                                        );
                                    } else {
                                        println!("Ran out of viewing rights");

                                        // Construct the path to the encrypted image
                                        let encrypted_image_path = format!("{}/{}_encrypted.png", images_folder, selected_image_id);

                                        // Check if the encrypted image exists
                                        if !Path::new(&encrypted_image_path).exists() {
                                            println!("Encrypted image file not found: {}", encrypted_image_path);
                                            return Ok(());
                                        }

                                        // Open the encrypted image using the system's default viewer
                                        println!("Displaying encrypted image: {}", encrypted_image_path);
                                        if let Err(e) = open_image_with_default_viewer(&encrypted_image_path).await {
                                            eprintln!("Error: {}", e);
                                        }

                                    }

                                    // Step 8: Save the updated JSON
                                    let mut file = File::create(json_image_views_path).expect("Failed to create images_views.json");
                                    let updated_json = serde_json::to_string_pretty(&Value::Object(image_views))
                                        .expect("Failed to serialize updated JSON");
                                    file.write_all(updated_json.as_bytes())
                                        .expect("Failed to write to images_views.json");

                                } else {
                                    println!("Invalid Image ID: {}", selected_image_id);
                                }



                                
                                
                            }
                            
                            "7" => {
                                // Multicast shutdown request
                                let shutdown_json = json!({
                                    "type": "shutdown",
                                    "user_id": user_id,
                                    "randam_number": random_num,
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
