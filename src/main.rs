use std::fs;
use std::io;
use std::path::Path;
use tokio::net::UdpSocket;
mod communication;
mod decode;

#[tokio::main]
async fn main() -> io::Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let directory_path = "/home/group05-f24/Desktop/Amr's Work/Images_dir";
    let dest_ip = "127.0.0.1";
    let request_port = 8080;

    // Send initial request to server
    let server_addr = format!("{}:{}", dest_ip, request_port);
    let mut buffer = [0u8; 2];
    socket.send_to(b"REQUEST", &server_addr).await?;
    println!("Sent request to server to send images");

    // Receive assigned port from server
    let (size, _) = socket.recv_from(&mut buffer).await?;
    if size == 2 {
        let port = u16::from_be_bytes([buffer[0], buffer[1]]);
        println!("Server assigned port: {}", port);

        // Iterate over images in the directory and send them
        for entry in fs::read_dir(directory_path)? {
            let image_path = entry?.path();
            let server_addr = format!("{}:{}", dest_ip, port);

            // Send image to server
            communication::send_image_over_udp(&socket, &image_path, &dest_ip, port).await?;
            println!("Image sent to server");

            // Receive encrypted image response
            let save_path = Path::new("../received_images/encrypted_image_received.png");
            communication::receive_encrypted_image(&socket, save_path).await?;
            println!("Encrypted image received and saved at {:?}", save_path);

            // Decode the encrypted image after receiving each one
            let decode_path :&str = "../received_images/encrypted_image_received.png";
            let save_decoded :&str= "../received_images/decrypted_image_received.png";

            match decode::decode_image(&decode_path, &save_decoded) {
                Ok(_) => println!("Decoded image saved at {:?}", save_decoded),
                Err(e) => eprintln!("Failed to decode image: {}", e),
            }
        }
    } else {
        eprintln!("Failed to receive a valid port from server");
    }

    Ok(())
}
