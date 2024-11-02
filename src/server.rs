use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use std::sync::Arc;
use std::net::SocketAddr;
use std::collections::{HashMap, HashSet};

use crate::config::{Message, MessageType};

pub struct ServerStats {
    pub self_addr: SocketAddr,
    pub pr: f32,
    pub peer_priorities: HashMap<SocketAddr, f32>, // Track each peer's priority
    pub client_request_queue: HashSet<String>, // Queue of pending client requests for this server
    pub peer_nodes: Vec<SocketAddr>, // List of peer nodes
    pub is_active: bool, // Whether the server is active
}

// Function to handle incoming heartbeat messages
pub async fn handle_heartbeat(
    sender_addr: SocketAddr,
    priority: f32,
    stats: &Arc<Mutex<ServerStats>>,
) {
    let mut state = stats.lock().await;

    // Only add/update the priority if the sender is a peer (not the server itself)
    if sender_addr != state.self_addr { // Assuming `server_addr` is the address of this server
        state.peer_priorities.insert(sender_addr, priority);
        // println!(
        //     "Received heartbeat from {} with priority {:.3}",
        //     sender_addr, priority
        // );
    }

    // for (addr, prio) in &state.peer_priorities {
    //     println!("Address: {}, Priority: {:.3}", addr, prio);
    // }

}


pub async fn process_client_request(
    id: u32,
    request_id: String,
    server_socket: Arc<UdpSocket>,
    stats: Arc<Mutex<ServerStats>>,
) {
    let mut state = stats.lock().await;

    // Check if the request has already been processed
    if state.client_request_queue.contains(&request_id) {
        println!(
            "[Server {}] Request ID: {} already handled or is currently being handled.",
            id, request_id
        );
        return;
    }

    // Clone necessary values from `state` while the lock is held
    let server_addr = state.self_addr;
    let server_priority = state.pr;
    let peer_nodes = state.peer_nodes.clone(); // Clone peers to use outside the lock

    // println!("Peer priorities and addresses:");
    for (addr, priority) in &state.peer_priorities {
        println!("Address: {}, Priority: {:.3}", addr, priority);
    }


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

    // // Find the highest priority among all peers, choosing the highest SocketAddr in case of a tie
    // let (highest_priority, highest_priority_server) = state
    //     .peer_priorities
    //     .iter()
    //     .max_by(|&(addr1, &priority1), &(addr2, &priority2)| {
    //         priority1
    //             .partial_cmp(&priority2)
    //             .unwrap_or(std::cmp::Ordering::Equal)
    //             // In case of equal priority, select the highest SocketAddr
    //             .then_with(|| addr1.cmp(addr2).reverse())
    //     })
    //     .map(|(addr, &priority)| (priority, addr))
    //     .unwrap_or((server_priority, &server_addr)); // Fallback to the server's own priority if no peers




    // Determine if the current server should be the coordinator
    // println!("HEEEEEEEEEEEEEEEEERE----------->{},{}", server_addr, *highest_priority_server);
    if(highest_priority_server == server_addr)
    {
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
            let encoded_msg = serde_cbor::ser::to_vec(&coordinator_msg).unwrap();
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

        // Handle client request directly
        handle_request(request_id.clone()).await;

        // Remove request ID from queue after handling
        let mut state = stats.lock().await;
        state.client_request_queue.remove(&request_id);
    } else {
        println!(
            "[Server {}] Not the coordinator for request ID: {}. My priority: {:.3}, Highest priority: {:.3}, Highest priority server IP: {}",
            id, request_id, server_priority, highest_priority, highest_priority_server
        );
    }
}


// Function to process a coordinator notification from another server
pub async fn handle_coordinator_notification(
    request_id: String,
    stats: Arc<Mutex<ServerStats>>,
) {
    let mut state = stats.lock().await;
    if state.client_request_queue.remove(&request_id) {
        println!(
            "Request ID: {} removed from queue as another server is handling it as coordinator",
            request_id
        );
    }
}

// Function to simulate handling the client request
async fn handle_request(request_id: String) {
    println!("[Coordinator] Handling request with ID: {}", request_id);
    // Add any request handling logic here, such as processing data or responding to the client
}
