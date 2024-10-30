use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Write};
use std::net::UdpSocket;

pub fn listen_for_image(port: u16, output_path: &str) -> io::Result<()> {
    let socket = UdpSocket::bind(("0.0.0.0", port))?;
    let mut buffer = vec![0u8; 1024];
    let mut file = File::create(output_path)?;
    let mut chunks = HashMap::new();
    let mut max_id = 0;

    println!("Listening on port {}", port);

    loop {
        // Receive data from the socket
        let (size, _src) = socket.recv_from(&mut buffer)?;
        
        // Break if the size is zero (end of transmission)
        if size == 0 {
            println!("End of transmission received.");
            break;
        }

        // Extract ID (first 4 bytes) and chunk data
        let chunk_id = u32::from_be_bytes(buffer[..4].try_into().unwrap()) as usize;
        let chunk_data = &buffer[4..size];
        
        // Store chunk data in HashMap with chunk ID as key
        chunks.insert(chunk_id, chunk_data.to_vec());
        max_id = max_id.max(chunk_id);

        println!("Received chunk ID {}, size {} bytes.", chunk_id, size - 4);
    }

    // Write chunks in order to the file
    for i in 0..=max_id {
        if let Some(chunk) = chunks.get(&i) {
            file.write_all(chunk)?;
        } else {
            println!("Warning: Missing chunk ID {}", i);
        }
    }

    println!("Image received and saved to {}", output_path);
    Ok(())
}
