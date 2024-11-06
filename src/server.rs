use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use std::sync::Arc;
use std::net::SocketAddr;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime};
use std::thread::sleep;

use crate::config::{Message, MessageType};

pub struct ServerStats {
    pub self_addr: SocketAddr,
    pub pr: f32,
    pub peer_priorities: HashMap<SocketAddr, f32>, // Track each peer's priority
    pub client_request_queue: HashSet<String>, // Queue of pending client requests for this server
    pub peer_nodes: Vec<SocketAddr>, // List of peer nodes
    pub is_active: bool, // Whether the server is active
    pub peer_alive: HashMap<SocketAddr, SystemTime>,
}

// Function to handle incoming heartbeat messages
// Function to handle incoming heartbeat messages
pub async fn handle_heartbeat(
    sender_addr: SocketAddr,
    priority: f32,
    stats: &Arc<Mutex<ServerStats>>,
) {
    let mut state = stats.lock().await;

    // Only add/update the priority if the sender is a peer (not the server itself)
    if sender_addr != state.self_addr {
        state.peer_priorities.insert(sender_addr, priority);
        // println!(
        //     "Updated priority from server {}: priority = {:.3}",
        //     sender_addr, priority
        // );

        state.peer_alive.insert(sender_addr, SystemTime::now());
    }

    // // Optional: Debug output to log the current state of peer priorities
    // for (addr, prio) in &state.peer_priorities {
    //     println!("Peer Address: {}, Priority: {:.3}", addr, prio);
    // }
}



pub async fn process_client_request(
    id: u32,
    request_id: String,
    server_socket: Arc<UdpSocket>,
    stats: Arc<Mutex<ServerStats>>,
) -> bool {
    let mut state = stats.lock().await;

    // Check if the request has already been processed
    if state.client_request_queue.contains(&request_id) {
        println!(
            "[Server {}] Request ID: {} already handled or is currently being handled.",
            id, request_id
        );
        return false;
    }

    // Clone necessary values from `state` while the lock is held
    let server_addr = state.self_addr;
    let server_priority = state.pr;
    let peer_nodes = state.peer_nodes.clone(); // Clone peers to use outside the lock

    // Evaluate peer priorities and determine if this server is the highest priority
    let mut highest_priority = server_priority;
    let mut highest_priority_server = server_addr;
    for (addr, &priority) in &state.peer_priorities {
        if priority > highest_priority {
            // Update if this peer has a higher priority
            highest_priority = priority;
            highest_priority_server = *addr;
        } else if priority == highest_priority {
            // If priorities are equal, choose the peer with the higher IP address
            if addr > &highest_priority_server {
                highest_priority_server = *addr;
            }
        }
    }

    // Determine if the current server should be the coordinator
    if highest_priority_server == server_addr {
        // Add request ID to the queue as it is being processed
        state.client_request_queue.insert(request_id.clone());
        drop(state); // Release lock to allow other operations while processing

        println!(
            "[Server {}] I am the coordinator for request ID: {}, with priority: {:.3} and IP: {}",
            id, request_id, server_priority, server_addr
        );

        // Notify peers that this server is handling the request as the coordinator
        for peer in peer_nodes {
            let coordinator_msg = Message {
                sender: server_addr,
                receiver: peer,
                msg_type: MessageType::CoordinatorNotification(request_id.clone()),
                payload: None,
            };
            let encoded_msg = serde_cbor::to_vec(&coordinator_msg).unwrap();
            let socket_clone = server_socket.clone();
            let rid = request_id.clone();

            tokio::spawn(async move {
                socket_clone.send_to(&encoded_msg, peer).await.unwrap();
                println!(
                    "Sent Coordinator notification for request {} from server {} to {}",
                    rid, id, peer
                );
            });
        }

        // This server is the coordinator
        true
    } else {
        // This server is not the coordinator
        println!(
            "[Server {}] Not the coordinator for request ID: {}. My priority: {:.3}, Highest priority: {:.3}, Highest priority server IP: {}",
            id, request_id, server_priority, highest_priority, highest_priority_server
        );
        false
    }
}


pub async fn handle_coordinator_notification(
    request_id: String,
    stats: Arc<Mutex<ServerStats>>,
) {
    let mut state = stats.lock().await;

    // Insert the request ID into the queue to mark it as being handled by the coordinator
    if state.client_request_queue.insert(request_id.clone()) {
        println!(
            "Request ID: {} added to queue, as it is being handled by the coordinator.",
            request_id
        );
    } else {
        println!(
            "Request ID: {} was already in the queue, no duplicate insertion.",
            request_id
        );
    }
}
