use tokio::sync::Mutex;
use std::sync::Arc;
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::time::{Duration, sleep};
use sysinfo::{System, SystemExt, CpuRefreshKind, RefreshKind};
use serde_cbor;
use std::collections::{HashMap, HashSet};
use rand::Rng;
use rand::rngs::StdRng;
use rand::SeedableRng;

mod config; // Assuming `config` contains `Message`, `MessageType` definitions.
mod server;
use server::{ServerStats, process_client_request, handle_heartbeat};

#[tokio::main]
async fn main() {
    let server_addresses = vec![
        "127.0.0.1:8082".parse().unwrap(),
        "127.0.0.1:8084".parse().unwrap(),
        "127.0.0.1:8089".parse().unwrap(),
    ];
    
    // Launch each server task
    for (i, &server_address) in server_addresses.iter().enumerate() {
        let peers: Vec<SocketAddr> = server_addresses
            .iter()
            .cloned()
            .filter(|&addr| addr != server_address)
            .collect();
    
        println!("Server {} initialized with peers: {:?}", server_address, peers); // Verify peer_nodes
    
        let stats = Arc::new(Mutex::new(ServerStats {
            self_addr: server_address,
            pr: 0.0,
            peer_priorities: HashMap::new(),
            client_request_queue: HashSet::new(),
            peer_nodes: peers, // Should contain only other servers
            is_active: true,
        }));
    
        let stats_clone = stats.clone();
        tokio::spawn(start_server(i as u32, server_address, stats_clone));
    }
    

    // Delay to allow servers to exchange initial heartbeats before clients start sending requests
    sleep(Duration::from_secs(10)).await;

    // Launch simulated clients that will send multicast requests to servers
    for client_id in 0..3 {
        tokio::spawn(start_client(client_id, server_addresses.clone()));
    }

    // Keep the main function running
    loop {
        sleep(Duration::from_secs(60)).await;
    }
}


// Function to start a simulated server
async fn start_server(id: u32, address: SocketAddr, stats: Arc<Mutex<ServerStats>>) {
    let server_socket = Arc::new(UdpSocket::bind(address).await.unwrap());
    println!("Server {} listening on {}", id, address);

    // Spawn the heartbeat task
    let heartbeat_socket = server_socket.clone();
    let stats_heartbeat = stats.clone();
    tokio::spawn(async move {
        loop {
            send_heartbeat(id, heartbeat_socket.clone(), stats_heartbeat.clone()).await;
            sleep(Duration::from_secs(6)).await;
        }
    });

    loop {
        let mut buf = [0; 1024];
        let (len, addr) = server_socket.recv_from(&mut buf).await.unwrap();
        let packet = &buf[..len];

        // Process incoming heartbeat or client message
        let msg = serde_cbor::from_slice::<config::Message>(packet).unwrap();
        match msg.msg_type {
            config::MessageType::Heartbeat(priority) => {
                handle_heartbeat(addr, priority, &stats).await;
            }
            config::MessageType::ClientRequest(ref request_id) => {
                process_client_request(id, request_id.clone(), server_socket.clone(), stats.clone()).await;
            }
            _ => {}
        }
    }
}

// Function to start a simulated client that sends requests every 4 seconds
async fn start_client(client_id: u32, peer_nodes: Vec<SocketAddr>) {
    let client_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());

    loop {
        // Generate a unique request ID
        let request_id = format!("client-request-{}", client_id);

        // Create a client request message
        let client_request = config::Message {
            sender: client_socket.local_addr().unwrap(),
            receiver: "0.0.0.0:0".parse().unwrap(),
            msg_type: config::MessageType::ClientRequest(request_id.clone()),
            payload: Some(request_id.clone()),
        };

        let encoded_msg = serde_cbor::ser::to_vec(&client_request).unwrap();

        // Multicast the message concurrently to each server
        let mut tasks = vec![];
        for &server_address in &peer_nodes {
            let socket_clone = client_socket.clone();
            let msg = encoded_msg.clone();
            let cid = client_id;
            let rid = request_id.clone();

            let task = tokio::spawn(async move {
                socket_clone.send_to(&msg, server_address).await.unwrap();
                println!("[Client {}] Sent request with ID: {} to server at {}", cid, rid, server_address);
            });

            tasks.push(task);
        }

        // Wait for all multicast send operations to complete
        futures::future::join_all(tasks).await;

        // Wait for 4 seconds before the next request
        tokio::time::sleep(Duration::from_secs(4)).await;
    }
}




// Function to send heartbeat messages
async fn send_heartbeat(id: u32, server_socket: Arc<UdpSocket>, stats: Arc<Mutex<ServerStats>>) {
    let mut system = System::new_with_specifics(
        RefreshKind::new()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory(),
    );
    system.refresh_cpu();
    let priority = 1.0 / system.load_average().one as f32;

    // Update the server's own priority in stats
    {
        let mut state = stats.lock().await;
        state.pr = priority;
    }

    // Heartbeat message
    let heartbeat = config::Message {
        sender: server_socket.local_addr().unwrap(),
        receiver: "0.0.0.0:0".parse().unwrap(), // Placeholder for each server in loop
        msg_type: config::MessageType::Heartbeat(priority),
        payload: None,
    };
    let encoded_msg = serde_cbor::ser::to_vec(&heartbeat).unwrap();

    let state = stats.lock().await;
    for &peer_addr in &state.peer_nodes {
        let socket_clone = server_socket.clone();
        let msg = encoded_msg.clone();
        tokio::spawn(async move {
            socket_clone.send_to(&msg, peer_addr).await.unwrap();
            // println!("Sent heartbeat from server {} to {}", id, peer_addr);
        });
    }
    
}
