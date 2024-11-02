use tokio::net::UdpSocket;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::io;
use std::path::Path;
use tokio::task;
use std::net::SocketAddr;
mod communication;
mod encode;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listen_socket = UdpSocket::bind("0.0.0.0:8080").await?;
    println!("Server listening for initial requests on port 8080");

    // Set to track used ports and ensure they are freed after use
    let used_ports = Arc::new(Mutex::new(HashSet::new()));

    loop {
        let mut buffer = [0u8; 1024];
        let (size, client_addr) = listen_socket.recv_from(&mut buffer).await?;

        if size == 0 {
            println!("Received empty request from {}", client_addr);
            continue;
        }

        println!("Received request to send image from {}", client_addr);

        // Allocate a unique port for this client
        let client_port = communication::allocate_unique_port(&used_ports)?;
        println!("I am here now");
        let client_socket = UdpSocket::bind(("0.0.0.0", client_port)).await?;
        println!("I am here now2");
        // Inform the client of the allocated port
        listen_socket.send_to(&client_port.to_be_bytes(), client_addr).await?;
        println!("Allocated port {} for client {}", client_port, client_addr);

        // Handle the image transmission and response in a separate task
        let used_ports = Arc::clone(&used_ports);
        task::spawn(async move {
            if let Err(e) = communication::handle_client(client_socket, client_addr).await {
                eprintln!("Error handling client {}: {}", client_addr, e);
            }
            // Free the port after transmission
            communication::free_port(&used_ports, client_port);
        });
    }
}

