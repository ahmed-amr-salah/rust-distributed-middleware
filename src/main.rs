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


mod config;
mod server;
mod communication;
mod encode; // Assuming this module includes encryption functions

use server::{ServerStats, process_client_request, handle_heartbeat, handle_coordinator_notification};

// Constants
const SERVER_ID: u32 = 1;          // Modify this for each server instance
const HEARTBEAT_PORT: u16 = 8085;   // Port for both sending and receiving heartbeat messages


// Declare `used_ports` as a global, shared state outside `main`
lazy_static::lazy_static! {
    static ref used_ports: Arc<tokio::sync::Mutex<HashSet<u16>>> = Arc::new(Mutex::new(HashSet::new()));
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // let client_port = communication::allocate_unique_port(&used_ports).await?;
    let local_addr: SocketAddr = "10.7.19.204:8080".parse().unwrap();
    let peer_addresses = vec![
        "10.40.61.237:8085".parse().unwrap(),
        // "127.0.0.1:8085".parse().unwrap(),
    ];

    let stats = Arc::new(Mutex::new(ServerStats {
        self_addr: local_addr,
        pr: 0.0,
        peer_priorities: HashMap::new(),
        client_request_queue: HashSet::new(),
        peer_nodes: peer_addresses.clone(),
        is_active: true,
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
            sleep(Duration::from_secs(6)).await;
        }
    });

    // Main loop to handle client requests
    // let used_ports = Arc::new(Mutex::new(HashSet::<u16>::new())); // Track used ports for clients

    loop {
        //let client_port = communication::allocate_unique_port(&used_ports).await?; //Mario
        let client_socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?); //Mario
        let client_port = client_socket.local_addr()?.port();
         
        let mut buffer = [0u8; 1024];
        let (size, client_addr) = listen_socket.recv_from(&mut buffer).await?;
        
        if size == 0 {
            println!("Received empty request from {}", client_addr);
            //communication::free_port(&used_ports, client_port).await;
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
        let used_ports_clone = Arc::clone(&used_ports);
        let client_socket_clone = Arc::clone(&client_socket); // Arc clone, not UdpSocket clone


        tokio::spawn(async move {
            // Determine if this server should act as the coordinator for this request
            let is_coordinator = process_client_request(SERVER_ID, request_id.clone(), client_socket_clone.clone(), stats_clone.clone()).await;
        
            if is_coordinator {
                println!("This server is elected as coordinator for request {}", request_id);
        
                // Coordinator handles the client by invoking `handle_client`
                if let Err(e) = communication::handle_client(client_socket_clone, client_addr).await {
                    eprintln!("Error handling client {}: {}", client_addr, e);
                }
                // Free the port after transmission
                //communication::free_port(&used_ports_clone, client_port);
            } else {
                println!("Another server will handle the request {}", request_id);
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

    let mut state = stats.lock().await;
    state.pr = priority;

    let heartbeat_msg = config::Message {
        sender: state.self_addr,
        receiver: "0.0.0.0:0".parse().unwrap(),
        msg_type: config::MessageType::Heartbeat(priority),
        payload: None,
    };

    let encoded_msg = serde_cbor::to_vec(&heartbeat_msg).unwrap();

    for &peer_addr in &state.peer_nodes {
        let peer_heartbeat_addr = SocketAddr::new(peer_addr.ip(), HEARTBEAT_PORT);
        heartbeat_socket.send_to(&encoded_msg, peer_heartbeat_addr).await.unwrap();
        println!("Sent heartbeat to {}", peer_heartbeat_addr);
    }
}
