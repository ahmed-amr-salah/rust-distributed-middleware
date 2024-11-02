use serde_derive::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Message {
    pub sender: SocketAddr,
    pub receiver: SocketAddr,
    pub msg_type: MessageType,
    pub payload: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum MessageType {
    ClientRequest(String), // Changed to `String` to store request ID as a String
    Heartbeat(f32),        // New message type for sending priority in heartbeats
    CoordinatorNotification(String), // Coordinator notification message with request ID
    CoordinationIntent(String, f32), // Coordination intent message with request ID and priority
}
