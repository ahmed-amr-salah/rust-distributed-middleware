use std::fs;
use std::io;
use std::path::Path;
use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration};
use dotenv::dotenv;
use std::env;
mod communication;
mod decode;

#[tokio::main]
async fn main() -> io::Result<()> {
    // LOAD THE LOCAL VARIABLE
    dotenv().ok();
    
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let directory_path = env::var("IMAGE_DIR").expect("IMAGE_DIR is not set in .env");
    let request_port = env::var("LISTENING_PORT").expect("LISTENING_PORT is not set in .env");

    // Define the IPs and request port for the three servers
    let server_ips = [
        env::var("FIRST_SERVER_IP").expect("FIRST_SERVER_IP is not set in .env"),
        env::var("SECOND_SERVER_IP").expect("SECOND_SERVER_IP is not set in .env"),
    ];    
    let mut buffer = [0u8; 2];

    // Send "REQUEST" message to all three servers
    for server_ip in &server_ips {
        let server_addr = format!("{}:{}", server_ip, request_port);
        socket.send_to(b"REQUEST", &server_addr).await?;
        println!("Sent request to server at {}", server_addr);
    }

    // Wait for the first server to respond with an assigned port
    let mut assigned_port = None;
    for _ in 0..server_ips.len() {
        match timeout(Duration::from_secs(5), socket.recv_from(&mut buffer)).await {
            Ok(Ok((size, src))) if size == 2 => {
                assigned_port = Some((src, u16::from_be_bytes([buffer[0], buffer[1]])));
                println!("Received port assignment from server at {}", src);
                break;
            }
            _ => {
                println!("No response or invalid response from a server.");
            }
        }
    }

    // If no server responded, exit with an error
    if let Some((server_addr, port)) = assigned_port {
        println!("Proceeding with server at {}:{}", server_addr, port);

        // Iterate over images in the directory and send them
        for entry in fs::read_dir(directory_path)? {
            let image_path = entry?.path();

            // Send image to the selected server
            communication::send_image_over_udp(&socket, &image_path, server_addr.ip().to_string(), port).await?;
            println!("Image sent to server at {}", server_addr);

            // Receive encrypted image response
            let save_path = Path::new("/home/g6/hamza_work/distributed/rust-distributed-middleware/received_images/encrypted_image_received.png");
            communication::receive_encrypted_image(&socket, save_path).await?;
            println!("Encrypted image received and saved at {:?}", save_path);

            // Decode the encrypted image after receiving each one
            let decode_path = "/home/g6/hamza_work/distributed/rust-distributed-middleware/received_images/encrypted_image_received.png";
            let save_decoded = "/home/g6/hamza_work/distributed/rust-distributed-middleware/received_images/decrypted_image_received.png";

            match tokio::task::spawn_blocking(move || decode::decode_image(decode_path, save_decoded))
                .await
            {
                Ok(Ok(_)) => println!("Decoded image saved at {:?}", save_decoded),
                Ok(Err(e)) => eprintln!("Failed to decode image: {}", e),
                Err(join_err) => eprintln!("Failed to execute decode_image: {:?}", join_err),
            }
        }
        // Send the "CLIENT_OFFLINE" notification to the server
        let offline_message = b"CLIENT_OFFLINE";
        socket.send_to(offline_message, (server_addr.ip().to_string(), port)).await?;
        println!("Notified server at {}:{} that client is going offline.", server_addr, port);
    } else {
        eprintln!("No server assigned a port. Exiting.");
    }

    Ok(())
}
