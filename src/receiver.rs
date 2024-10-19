use std::fs::File;
use std::io::{self, Write};
use std::net::UdpSocket;

pub fn listen_for_image(port: u16, output_path: &str) -> io::Result<()> {
    let socket = UdpSocket::bind(("0.0.0.0", port))?;
    let mut buffer = vec![0u8; 1024];  // Buffer to hold incoming chunks
    let mut file = File::create(output_path)?;  // Create the output file

    println!("Listening on port {}", port);

    loop {
        // Receive a chunk of data from the UDP socket
        let (size, _src) = socket.recv_from(&mut buffer)?;

        // Check if the size is 0, which we'll treat as the termination of the transmission
        if size == 0 {
            println!("End of transmission received.");
            break;  // Exit the loop if transmission is complete
        }

        // Write the received chunk to the file
        file.write_all(&buffer[..size])?;
        println!("Received and wrote {} bytes.", size);
    }

    // After the loop, close the file and print a success message
    println!("Image received and saved to {}", output_path);
    Ok(())
}
