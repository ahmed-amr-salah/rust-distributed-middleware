use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::time::{Duration, sleep};
use mysql_async::Pool;
use sysinfo::{System, SystemExt};
use serde_cbor;
use std::time::SystemTime;

mod config;
mod server;
mod communication;
mod encode;

use server::{ServerStats, process_client_request, handle_heartbeat, handle_coordinator_notification};
use communication::{handle_client, handle_client_exit, allocate_unique_port, free_port};

// Constants
const SERVER_ID: u32 = 1;
const HEARTBEAT_PORT: u16 = 8085;
const HEARTBEAT_PERIOD: u64 = 3;

// Declare `used_ports` as a global, shared state outside `main`
lazy_static::lazy_static! {
    static ref USED_PORTS: Arc<Mutex<HashSet<u16>>> = Arc::new(Mutex::new(HashSet::new()));
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize database connection pool

    let db_pool = Arc::new(Pool::new(db_url));

    // Local server setup
    let local_addr: SocketAddr = "10.40.51.73:8081".parse().unwrap();
    let peer_addresses = vec![
        "10.7.19.204:8085".parse().unwrap(),
    ];

    let stats = Arc::new(Mutex::new(ServerStats {
        self_addr: local_addr,
        pr: -1.0,
        peer_priorities: HashMap::new(),
        client_request_queue: HashSet::new(),
        peer_nodes: peer_addresses.clone(),
        is_active: true,
        peer_alive: HashMap::new(),
    }));

    let listen_socket = UdpSocket::bind(local_addr).await?;
    println!("Server listening for initial client requests on {}", local_addr);

    // Shared heartbeat socket
    let heartbeat_socket = Arc::new(UdpSocket::bind(("0.0.0.0", HEARTBEAT_PORT)).await?);
    println!("Server listening for heartbeat messages on port {}", HEARTBEAT_PORT);

    // Spawn tasks for heartbeat handling
    let stats_clone = Arc::clone(&stats);
    let heartbeat_socket_clone = Arc::clone(&heartbeat_socket);
    tokio::spawn(async move {
        listen_for_heartbeats(heartbeat_socket_clone, stats_clone).await;
    });

    let stats_clone = Arc::clone(&stats);
    let heartbeat_socket_clone = Arc::clone(&heartbeat_socket);
    tokio::spawn(async move {
        loop {
            send_heartbeat(heartbeat_socket_clone.clone(), stats_clone.clone()).await;
            sleep(Duration::from_secs(HEARTBEAT_PERIOD)).await;
        }
    });

    // Main loop for handling client requests
    loop {
        let client_port = allocate_unique_port(&USED_PORTS).await?;
        let client_socket = Arc::new(UdpSocket::bind(("0.0.0.0", client_port)).await?);

        let mut buffer = [0u8; 1024];
        let (size, client_addr) = listen_socket.recv_from(&mut buffer).await?;
        if size == 0 {
            println!("Received empty request from {}", client_addr);
            continue;
        }

        // Notify client of its allocated port
        listen_socket.send_to(&client_port.to_be_bytes(), client_addr).await?;
        println!("Allocated port {} for client {}", client_port, client_addr);

        // Process client requests
        let stats_clone = Arc::clone(&stats);
        let client_socket_clone = Arc::clone(&client_socket);
        let db_pool_for_client = Arc::clone(&db_pool); // Clone for `handle_client`
        let db_pool_for_exit = Arc::clone(&db_pool); // Clone for `handle_client_exit`

        tokio::spawn(async move {
            // Check if this server is the coordinator
            let is_coordinator = process_client_request(
                SERVER_ID,
                format!("{}-{}-{:x}", client_addr.ip(), client_addr.port(), md5::compute(&buffer[..size])),
                client_socket_clone.clone(),
                stats_clone.clone(),
            )
            .await;

            if is_coordinator {
                println!("This server is elected as coordinator for client {}", client_addr);

                // Handle the client
                if let Err(e) = handle_client(client_socket_clone, client_addr, db_pool_for_client).await {
                    eprintln!("Error handling client {}: {}", client_addr, e);
                }

                // Mark client offline when done
                if let Err(e) = handle_client_exit(client_addr, db_pool_for_exit).await {
                    eprintln!("Error marking client {} offline: {}", client_addr, e);
                }

                // Free the port
                free_port(&USED_PORTS, client_port).await;
            } else {
                println!("Another server will handle the request for {}", client_addr);
            }
        });
    }
}

// Function to listen for heartbeat messages on HEARTBEAT_PORT
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

    let (self_addr, peer_nodes, peer_alive) = {
        let mut state = stats.lock().await;
        state.pr = priority;
        state.peer_priorities.retain(|_, &mut prio| prio != -1.0);
        (
            state.self_addr,
            state.peer_nodes.clone(),
            state.peer_alive.clone(),
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
        if let Some(last_alive_time) = peer_alive.get(&peer_addr) {
            if let Ok(duration) = SystemTime::now().duration_since(*last_alive_time) {
                if duration >= Duration::from_secs((3 * HEARTBEAT_PERIOD) / 2) {
                    let mut state = stats.lock().await;
                    state.peer_priorities.insert(peer_addr, -1.0);
                }
            }
        }

        let peer_heartbeat_addr = SocketAddr::new(peer_addr.ip(), HEARTBEAT_PORT);

        if let Err(e) = heartbeat_socket.send_to(&encoded_msg, peer_heartbeat_addr).await {
            eprintln!("Failed to send heartbeat to {}: {:?}", peer_heartbeat_addr, e);
        } else {
            println!("Sent heartbeat to {}", peer_heartbeat_addr);
        }
    }
}
