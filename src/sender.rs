use std::fs;
use std::io;
use std::net::UdpSocket;

pub fn send_image_over_udp(image_path: &str, dest_ip: &str, port: u16) -> io::Result<()> {
    // Bind to any available port for sending
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    let image_data = fs::read(image_path)?;  // Read the entire image file into a byte vector

    let chunk_size = 1024;  // Size of each chunk in bytes
    let total_chunks = (image_data.len() + chunk_size - 1) / chunk_size; // Calculate total chunks

    for (i, chunk) in image_data.chunks(chunk_size).enumerate() {
        socket.send_to(chunk, (dest_ip, port))?;  // Send each chunk to the specified destination
        println!("Sending chunk {}/{}", i + 1, total_chunks);
    }

    // Optionally send an empty packet to signal the end of transmission
    socket.send_to(&[], (dest_ip, port))?; // Sending a zero-size packet

    Ok(())
}
