use std::fs;
use std::io;
use std::net::UdpSocket;

pub fn send_image_over_udp(image_path: &str, dest_ip: &str, port: u16) -> io::Result<()> {
    // Bind to any available port for sending
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    let image_data = fs::read(image_path)?;  // Read the entire image file into a byte vector

    let chunk_size = 1024;
    let id_size = 4; // Size of the ID in bytes
    let data_chunk_size = chunk_size - id_size; // Adjust for the ID space
    let total_chunks = (image_data.len() + data_chunk_size - 1) / data_chunk_size;

    for (i, chunk) in image_data.chunks(data_chunk_size).enumerate() {
        // Create a buffer to hold ID + chunk
        let mut packet = Vec::with_capacity(chunk_size);

        // Add the ID to the packet (as 4 bytes)
        packet.extend_from_slice(&(i as u32).to_be_bytes());
        
        // Add the actual chunk data
        packet.extend_from_slice(chunk);

        // Send packet with ID
        socket.send_to(&packet, (dest_ip, port))?;
        println!("Sending chunk {}/{}", i + 1, total_chunks);
    }

    // Optionally send a zero-size packet to signal end of transmission
    socket.send_to(&[], (dest_ip, port))?;

    Ok(())
}
