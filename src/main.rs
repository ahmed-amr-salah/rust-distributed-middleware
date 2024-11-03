use std::fs;
use std::io;
use std::path::Path;
use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration};
mod communication;
mod decode;

#[tokio::main]
async fn main() -> io::Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let directory_path = "/home/g6/Desktop/Hamza's_Work/dist3/Images_dir";
    let request_port = 8080;

    // Define the IPs and request port for the three servers
    let server_ips = ["10.40.61.237", "10.7.19.204"];
    let mut buffer = [0u8; 2];

    // Send "REQUEST" message to all three servers
    for &server_ip in &server_ips {
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
            let save_path = Path::new("../received_images/encrypted_image_received.png");
            communication::receive_encrypted_image(&socket, save_path).await?;
            println!("Encrypted image received and saved at {:?}", save_path);

            // Decode the encrypted image after receiving each one
            let decode_path = "../received_images/encrypted_image_received.png";
            let save_decoded = "../received_images/decrypted_image_received.png";

            match decode::decode_image(decode_path, save_decoded) {
                Ok(_) => println!("Decoded image saved at {:?}", save_decoded),
                Err(e) => eprintln!("Failed to decode image: {}", e),
            }
        }
    } else {
        eprintln!("No server assigned a port. Exiting.");
    }

    Ok(())
}
