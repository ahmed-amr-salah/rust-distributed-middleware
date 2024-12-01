use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::time::{Duration, sleep};
use std::collections::{HashMap, HashSet};
use serde_cbor;
use serde_json::json;
use serde_json::Value;
use std::io;
use sysinfo::{System, SystemExt};
use rand::Rng;
use rand::rngs::StdRng;
use rand::SeedableRng;
use tokio::sync::Mutex;
use std::sync::Arc;
use std::net::UdpSocket as StdUdpSocket;
use std::time::SystemTime;
use std::env;
use dotenv::dotenv;
use mysql::Pool;


mod config;
mod server;
mod communication;
mod encode; // Assuming this module includes encryption functions
mod DOS;

use server::{ServerStats, process_client_request, handle_heartbeat, handle_coordinator_notification};
use DOS::{register_user, sign_in_user, get_images_up, shutdown_client, insert_into_resources, update_access_rights, get_resources_by_client_id};
// Constants
const SERVER_ID: u32 = 1;          // Modify this for each server instance
const HEARTBEAT_PORT: u16 = 8085;   // Port for both sending and receiving heartbeat messages
const HEARTBEAT_PERIOD: u64 = 2;



#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    // Server settings
    let local_addr: SocketAddr = "10.7.16.48:8081".parse().unwrap();
    let peer_addresses = vec![
        // "10.40.51.73:8085".parse().unwrap(),
        // "10.7.19.18:8085".parse().unwrap(),
    ];

    // Connect to the database
    dotenv().ok();
    let db_url = env::var("db_url").expect("DATABASE_URL must be set");
    let pool = Pool::new(db_url.as_str())?;
    let mut conn = pool.get_conn()?;
    let conn = Arc::new(Mutex::new(conn));
    println!("Connected to the DoS successfully");

    let stats = Arc::new(Mutex::new(ServerStats {
        self_addr: local_addr,
        pr: 0.0,
        peer_priorities: HashMap::new(),
        client_request_queue: HashSet::new(),
        peer_nodes: peer_addresses.clone(),
        is_active: true,
        peer_alive: HashMap::new(),
    }));

    let listen_socket = Arc::new(UdpSocket::bind(local_addr).await?);
    println!("Server listening for initial client requests on {}", local_addr);

    // Shared heartbeat socket for sending and receiving on HEARTBEAT_PORT
    let heartbeat_socket = Arc::new(UdpSocket::bind(("0.0.0.0", HEARTBEAT_PORT)).await?);
    println!("Server listening for heartbeat messages on port {}", HEARTBEAT_PORT);

    // Start listening for heartbeat messages
    let stats_clone = Arc::clone(&stats);
    let heartbeat_socket_clone = Arc::clone(&heartbeat_socket);
    tokio::spawn(async move {
        listen_for_heartbeats(heartbeat_socket_clone, stats_clone).await;
    });

    // Start sending heartbeats to peers
    let stats_clone = Arc::clone(&stats);
    let heartbeat_socket_clone = Arc::clone(&heartbeat_socket);
    tokio::spawn(async move {
        loop {
            send_heartbeat(heartbeat_socket_clone.clone(), stats_clone.clone()).await;
            sleep(Duration::from_secs(HEARTBEAT_PERIOD)).await;
        }
    });
    
    
    // Main loop to handle client requests
    loop {
        let client_socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
        let client_port = client_socket.local_addr()?.port();
        
        let mut buffer = [0u8; 1024];
        let (size, client_addr) = listen_socket.recv_from(&mut buffer).await?;
        
        if size == 0 {
            println!("Received empty request from {}", client_addr);
            // communication::free_port(&used_ports, client_port).await;
            continue;
        }
        
        println!("Received request from client ---------------------> {}", client_addr);
        
        let mut request_id = format!(
            "{}-{}-{:x}", 
            client_addr.ip(),
            client_addr.port(),
            md5::compute(&buffer[..size]) // Ensure request ID is unique
        );

        let json = String::from_utf8_lossy(&buffer[..size]).to_string();
        if json.contains("randam_number"){
            let parsed_json: serde_json::Value = serde_json::from_str(&json).unwrap();
            let random_number = parsed_json["randam_number"].as_i64().unwrap();
            request_id = format!("{}{}", request_id, random_number);
            println!("Received random number: {}", random_number);
            println!("Concatenated Result: {}", request_id);
        }

        
        
        let stats_clone = Arc::clone(&stats);
        let client_socket_clone = Arc::clone(&client_socket); // Arc clone, not UdpSocket clone
        
        
        let listen_socket_clone = Arc::clone(&listen_socket);
        let conn_clone = Arc::clone(&conn);
        tokio::spawn(async move {
            // Determine if this server should act as the coordinator for this request
            let is_coordinator = process_client_request(SERVER_ID, request_id.clone(), client_socket_clone.clone(), stats_clone.clone()).await;
            
            if is_coordinator {
                println!("This server is elected as coordinator for request {}", request_id);
                
                // Types of Requests
                // 1. Client Registration
                if json.contains("register") {
                    println!("Received request from client {}", String::from_utf8_lossy(&buffer[..size]));
                    println!("Client registration request received");
                    let mut dos_conn = conn_clone.lock().await;
                    let client_addr_clone = client_addr.clone();
                    let response = register_user(&mut dos_conn, &client_addr_clone.to_string());
                    println!("Response: {}", response);
                    let response_bytes = serde_json::to_vec(&response).unwrap();  
                    println!("This is the client Address I am sending to ------>: {}",client_addr);                
                    if let Err(e) = async {
                        listen_socket_clone
                            .send_to(&response_bytes, client_addr)
                            .await?;
                        Ok::<(), io::Error>(()) // Explicitly define the `Result` type
                    }
                    .await
                    {
                        eprintln!("Error in spawned task: {}", e);
                    }
                }
                
                // 2. Client Sign-in
                else if json.contains("sign_in") {
                    println!("Received request from client {}", String::from_utf8_lossy(&buffer[..size]));
                    println!("Client sign-in request received");
                    let json_response: serde_json::Value = match serde_json::from_slice(&buffer[..size]) {
                        Ok(json) => json,
                        Err(e) => {
                            eprintln!("Failed to parse JSON: {}", e);
                            return;
                        }
                    };
                    let client_id = json_response["user_id"].as_u64().unwrap();
                    let p2p_socket = json_response["p2p_socket"].as_str().unwrap_or("").to_string();

                    let mut dos_conn = conn_clone.lock().await;
                    let client_addr_clone = client_addr.clone();
                    let p2p_socket_clone = p2p_socket.clone();

                    // Split client_addr_clone into IP and port
                    let client_ip = client_addr_clone.ip().to_string();
                    

                    // Split p2p_socket_clone into IP and port
                    let p2p_socket_parts: Vec<&str> = p2p_socket_clone.split(':').collect();
                    let p2p_port = p2p_socket_parts[1]; // Port from the p2p socket

                    // Concatenate the client_ip with p2p_port to create the new socket address string
                    let combined_socket_str = format!("{}:{}", client_ip, p2p_port);

                    let response = sign_in_user(&mut dos_conn, &client_id, &combined_socket_str);

                    // name json that has the images and the views the user receveid while he is offlice 
                    let offline_requests = get_resources_by_client_id(&mut dos_conn, client_id);

                    let mut response_bytes = serde_json::to_vec(&response).unwrap();
                    if response["status"] == "success"{
                        response_bytes = serde_json::to_vec(&offline_requests).unwrap();
                    }

                    if let Err(e) = async {
                        listen_socket_clone
                            .send_to(&response_bytes, client_addr)
                            .await?;
                        Ok::<(), io::Error>(()) // Explicitly define the `Result` type
                    }
                    .await
                    {
                        eprintln!("Error in spawned task: {}", e);
                    }
                }
                
                
                // 3. Client asks who is up and has what
                else if json.contains("active_users") {
                    println!("Client who_is_up request received");
                    println!("Received request from client {}", String::from_utf8_lossy(&buffer[..size]));
                    let mut dos_conn = conn_clone.lock().await;
                    let json_response: serde_json::Value = match serde_json::from_slice(&buffer[..size]) {
                        Ok(json) => json,
                        Err(e) => {
                            eprintln!("Failed to parse JSON: {}", e);
                            return;
                        }
                    };
                    let client_id = json_response["user_id"].as_u64().unwrap();
                    let response = get_images_up(&mut dos_conn, &client_id);
                    println!("Response: {}", response);
                    let response_bytes = serde_json::to_vec(&response).unwrap();
                    if let Err(e) = async {
                        listen_socket_clone
                            .send_to(&response_bytes, client_addr)
                            .await?;
                        Ok::<(), io::Error>(()) // Explicitly define the `Result` type
                    }
                    .await
                    {
                        eprintln!("Error in spawned task: {}", e);
                    }
                }


                // 4. Client Shutdown
                else if json.contains("shutdown") {
                    println!("Client shutdown request received");
                    println!("Received request from client {}", String::from_utf8_lossy(&buffer[..size]));
                    let json_response: serde_json::Value = match serde_json::from_slice(&buffer[..size]) {
                        Ok(json) => json,
                        Err(e) => {
                            eprintln!("Failed to parse JSON: {}", e);
                            return;
                        }
                    };
                    let client_id = json_response["user_id"].as_u64().unwrap();
                    let mut dos_conn = conn_clone.lock().await;
                    let response = shutdown_client(&mut dos_conn, client_id);
                    println!("Response: {}", response);
                    let response_bytes = serde_json::to_vec(&response).unwrap();
                    if let Err(e) = async {
                        listen_socket_clone
                            .send_to(&response_bytes, client_addr)
                            .await?;
                        Ok::<(), io::Error>(()) // Explicitly define the `Result` type
                    }
                    .await
                    {
                        eprintln!("Error in spawned task: {}", e);
                    }

                }
                // 5. client wants to report that another peer went offline and did not receive the change view request
                
                // else if json.contains("change-view") 
                //     {
                //         // Parse the incoming JSON
                //         let parsed_json: serde_json::Value = serde_json::from_str(&json).unwrap();
                //         println!("Received change view request from client {}", String::from_utf8_lossy(&buffer[..size]));

                //         // Extract required fields from the JSON
                //         let viewer_IP = parsed_json["peer_address"].as_str().unwrap_or("");
                //         let image_id = parsed_json["image_id"].as_str().unwrap_or("");
                //         let views = parsed_json["requested_views"].as_i64().unwrap_or(0) as i32;
                        
                //         println!("Viewer IP: {}", viewer_IP);
                //         println!("Image ID: {}", image_id);
                //         println!("Requested Views: {}", views);

                //         // Use the add_client_image_entry function to insert into the client_images table
                //         let mut dos_conn = conn_clone.lock().await;
                //         let add_entry_response = update_access_rights(&mut dos_conn, viewer_IP, image_id, views);

                //         if add_entry_response["status"] == "failure" {
                //             println!("Failed to add client image entry: {}", add_entry_response["error"]);
                //         } else {
                //             println!("Successfully added client image entry.");
                //         }

                //         let viewer_id = add_entry_response["viewer_id"].as_u64().unwrap_or(0);

                //         // Use the shutdown_client function to mark the client as offline
                //         let shutdown_response = shutdown_client(&mut dos_conn, viewer_id);

                //         if shutdown_response["status"] == "failure" {
                //             println!("Failed to mark client as offline: {}", shutdown_response["error"]);
                //         } else {
                //             println!("Successfully marked client as offline.");
                //         }
                //     }

                else if json.contains("change-view") 
                {
                    // Parse the incoming JSON
                    let parsed_json: serde_json::Value = serde_json::from_str(&json).unwrap();
                    println!("Received change view request from client {}", String::from_utf8_lossy(&buffer[..size]));

                    // Extract required fields from the JSON
                    let viewer_IP = parsed_json["peer_address"].as_str().unwrap_or("");
                    let image_id = parsed_json["image_id"].as_str().unwrap_or("");
                    let views = parsed_json["requested_views"].as_i64().unwrap_or(0) as i32;

                    // Log missing fields
                    if viewer_IP.is_empty() {
                        println!("Warning: peer_address is missing in the request.");
                    }
                    if image_id.is_empty() {
                        println!("Warning: image_id is missing in the request.");
                    }
                    if views == 0 {
                        println!("Warning: requested_views is 0 or missing in the request.");
                    }

                    println!("Viewer IP: {}", viewer_IP);
                    println!("Image ID: {}", image_id);
                    println!("Requested Views: {}", views);

                    // Use the add_client_image_entry function to insert into the client_images table
                    let mut dos_conn = conn_clone.lock().await;
                    let add_entry_response = update_access_rights(&mut dos_conn, viewer_IP, image_id, views);

                    if add_entry_response["status"] == "failure" {
                        println!("Failed to add client image entry: {}", add_entry_response["error"]);
                    } else {
                        println!("Successfully added client image entry.");
                    }

                    // Extract the viewer_id from add_entry_response
                    let viewer_id = add_entry_response["viewer_id"].as_u64().unwrap_or(0);

                    // Ensure viewer_id is valid before proceeding
                    if viewer_id == 0 {
                        println!("Invalid viewer ID: cannot mark client as offline.");
                    } else {
                        // Use the shutdown_client function to mark the client as offline
                        let shutdown_response = shutdown_client(&mut dos_conn, viewer_id);

                        if shutdown_response["status"] == "failure" {
                            println!("Failed to mark client as offline: {}", shutdown_response["error"]);
                        } else {
                            println!("Successfully marked client as offline.");
                        }
                    }
                }


                // 6. Client requests for image encryption
                else {
                   
                    let client_message: Vec<&str> = json.split(',').collect();
                    let client_id:u64= client_message[0].parse::<u64>().unwrap();
                    let image_id = client_message[1];
                  
                    println!("Client encryption request received");
                    // Send the assigned client port back to the client
                    if let Err(e) = async {
                        listen_socket_clone
                        .send_to(&client_port.to_be_bytes(), client_addr)
                        .await?;
                        Ok::<(), io::Error>(()) // Explicitly define the `Result` type
                    }
                    .await
                    {
                        eprintln!("Error in spawned task: {}", e);
                    }
                    println!("Allocated port {} for client {}", client_port, client_addr);
                    
                    // Coordinator handles the client by invoking `handle_client`
                    if let Err(e) = communication::handle_client(client_socket_clone, client_addr).await {
                        eprintln!("Error handling client {}: {}", client_addr, e);
                    }

                    // Adding the client resource to the dos
                    let mut dos_conn = conn_clone.lock().await;
                    insert_into_resources(&mut dos_conn, client_id, &image_id);
                }

                
    // No return, function just runs the insert
    drop(client_socket);
                
            } else {
                println!("Another server will handle the re10.7.19.18-58052quest {}", request_id);
            }pub async fn handle_heartbeat(
                sender_addr: SocketAddr,
                priority: f32,
                stats: &Arc<Mutex<ServerStats>>,
            ) {
                let mut state = stats.lock().await;
            
                // Only add/update the priority if the sender is a peer (not the server itself)
                if sender_addr != state.self_addr { // Assuming `server_addr` is the address of this server
                    state.peer_priorities.insert(sender_addr, priority);
                }    
            }
        });
        
    }
}

// Dedicated function to listen for heartbeat messages on HEARTBEAT_PORT
async fn listen_for_heartbeats(heartbeat_socket: Arc<UdpSocket>, stats: Arc<Mutex<ServerStats>>) {
    let mut buffer = [0u8; 1024];

    loop {
        let (size, sender_addr) = heartbeat_socket.recv_from(&mut buffer).await.unwrap();

        if size > 0 {
            let msg: config::Message = serde_cbor::from_slice(&buffer[..size]).unwrap();
            match msg.msg_type {
                config::MessageType::Heartbeat(priority) => {
                    handle_heartbeat(sender_addr, priority, &stats).await;
                    println!(
                        "Updated priority from server {}: priority = {:.3}",
                        sender_addr, priority
                    );
                }
                config::MessageType::CoordinatorNotification(request_id) => {
                    // Handle incoming coordinator notifications
                    handle_coordinator_notification(request_id, stats.clone()).await;
                }
                _ => {
                    println!("Received unknown message type from {}", sender_addr);
                }
            }
        }
    }
}

// Function to send heartbeat messages to peers on HEARTBEAT_PORT

async fn send_heartbeat(heartbeat_socket: Arc<UdpSocket>, stats: Arc<Mutex<ServerStats>>) {
    let mut system = System::new_all();
    system.refresh_cpu();
    let load_average = system.load_average().one;
    let priority = 1.0 / load_average as f32;

    // Gather and update stats in a single lock scope
    let (self_addr, peer_nodes, peer_alive) = {
        let mut state = stats.lock().await;
        state.pr = priority;
        state.peer_priorities.retain(|_, &mut prio| prio != -1.0); // Remove any offline peers
        (
            state.self_addr,
            state.peer_nodes.clone(),  // Clone peer nodes to avoid locking while iterating
            state.peer_alive.clone(),  // Clone peer last alive times
        )
    };

    let heartbeat_msg = config::Message {
        sender: self_addr,
        receiver: "0.0.0.0:0".parse().unwrap(),
        msg_type: config::MessageType::Heartbeat(priority),
        payload: None,
    };

    let encoded_msg = serde_cbor::to_vec(&heartbeat_msg).unwrap();

    for &peer_addr in &peer_nodes {     
        // Check the last alive time for each peer without locking again
        let mut state = stats.lock().await;
        // if let Some(&priority) = state.peer_priorities.get(&peer_addr) {
        //     if priority <= 0.0 {
        //         continue;
        //     }
        // }        
        if let Some(last_alive_time) = peer_alive.get(&peer_addr) {
            if let Ok(duration) = SystemTime::now().duration_since(*last_alive_time) {
                if duration >= Duration::from_secs((3 * HEARTBEAT_PERIOD) / 2) {
                    state.peer_priorities.insert(peer_addr, 0.0);
                    // println!("Updated priority for {}: {:?}", peer_addr, state.peer_priorities.get(&peer_addr));
                }
            }
        }

        let peer_heartbeat_addr = SocketAddr::new(peer_addr.ip(), HEARTBEAT_PORT);
        
        // Send heartbeat
        if let Err(e) = heartbeat_socket.send_to(&encoded_msg, peer_heartbeat_addr).await {
            eprintln!("Failed to send heartbeat to {}: {:?}", peer_heartbeat_addr, e);
            continue;
        }

        println!("Sent heartbeat to {}", peer_heartbeat_addr);
    }
}

