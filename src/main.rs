use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::time::{Duration, sleep};
use std::collections::{HashMap, HashSet};
use serde_cbor;
use std::io;
use sysinfo::{System, SystemExt};
use rand::Rng;
use rand::rngs::StdRng;
use rand::SeedableRng;
use tokio::sync::Mutex;
use std::sync::Arc;
use std::net::UdpSocket as StdUdpSocket;
use std::time::SystemTime;


mod config;
mod server;
mod communication;
mod encode; // Assuming this module includes encryption functions

use server::{ServerStats, process_client_request, handle_heartbeat, handle_coordinator_notification};

// Constants
const SERVER_ID: u32 = 1;          // Modify this for each server instance
const HEARTBEAT_PORT: u16 = 8085;   // Port for both sending and receiving heartbeat messages
const HEARTBEAT_PERIOD: u64 = 3;


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // let client_port = communication::allocate_unique_port(&used_ports).await?;
    let local_addr: SocketAddr = "10.7.19.204:8081".parse().unwrap();
    let peer_addresses = vec![
        "10.7.19.205:8085".parse().unwrap(),
        // "127.0.0.1:8085".parse().unwrap(),
    ];

    let stats = Arc::new(Mutex::new(ServerStats {
        self_addr: local_addr,
        pr: 0.0,
        peer_priorities: HashMap::new(),
        client_request_queue: HashSet::new(),
        peer_nodes: peer_addresses.clone(),
        is_active: true,
        peer_alive: HashMap::new(),
    }));

    let listen_socket = UdpSocket::bind(local_addr).await?;
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
        // let client_port = communication::allocate_unique_port(&used_ports).await?; //Mario
        // let client_socket = Arc::new(UdpSocket::bind(("0.0.0.0", client_port)).await?); //Mario
        let client_socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
        let client_port = client_socket.local_addr()?.port();

        let mut buffer = [0u8; 1024];
        let (size, client_addr) = listen_socket.recv_from(&mut buffer).await?;
        
        if size == 0 {
            println!("Received empty request from {}", client_addr);
            // communication::free_port(&used_ports, client_port).await;
            continue;
        }
        
        println!("Received request from client {}", client_addr);
        
        // let client_socket = Arc::new(UdpSocket::bind(("0.0.0.0", client_port)).await?);
        println!("---------------------> {}", client_port);
        listen_socket.send_to(&client_port.to_be_bytes(), client_addr).await?;
        println!("Allocated port {} for client {}", client_port, client_addr);

        let request_id = format!(
            "{}-{}-{:x}", 
            client_addr.ip(),
            client_addr.port(),
            md5::compute(&buffer[..size]) // Ensure request ID is unique
        );

        let stats_clone = Arc::clone(&stats);
        let client_socket_clone = Arc::clone(&client_socket); // Arc clone, not UdpSocket clone


        tokio::spawn(async move {
            // Determine if this server should act as the coordinator for this request
            let is_coordinator = process_client_request(SERVER_ID, request_id.clone(), client_socket_clone.clone(), stats_clone.clone()).await;
        
            if is_coordinator {
                println!("This server is elected as coordinator for request {}", request_id);

                // print the content of the packet the we received as string not as bytes
                let json = String::from_utf8_lossy(&buffer[..size]);
                println!("Received request from client {}", String::from_utf8_lossy(&buffer[..size]));
                
                // Types of Requests
                // 1. Client Registration
                if json.contains("register") {
                    println!("Client registration request received");
                }

                // 2. Client Sign-in
                else if json.contains("sign_in") {
                    println!("Client sign-in request received");
                }

                // 2. Client Shutdown
                else if json.contains("shutdown") {
                    println!("Client shutdown request received");
                }

                // 3. Client requests for image encryption
                else {
                    println!("Client encryption request received");
                
                    // Coordinator handles the client by invoking `handle_client`
                    if let Err(e) = communication::handle_client(client_socket_clone, client_addr).await {
                        eprintln!("Error handling client {}: {}", client_addr, e);
                    }
                }

                // Free the port after transmission
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
                    state.peer_priorities.insert(peer_addr, -1.0);
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

