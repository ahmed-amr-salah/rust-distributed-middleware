// declare the modules
mod sender;
mod receiver;

// use the required libraries
use std::io;
use std::thread;

fn main() {
    println!("+++++++ Rust Distributed Middleware Client +++++++");
    println!("+++++++ Enter the image path to send over UDP +++++++");
    
    // Read image path from user input
    let mut image_path = String::new();
    io::stdin().read_line(&mut image_path).expect("Failed to read input");
    let image_path = image_path.trim().to_string();  // Trim and store the input path as a string

    // Define the destination IP and port for sending and receiving
    let dest_ip = "127.0.0.1";  // You can change this to the actual destination IP
    let port = 8079;
    let output_path = "received_image.png";  // Path to save the received image
    
    // Spawn the sender thread
    let sender_thread = thread::spawn(move || {
        if let Err(e) = sender::send_image_over_udp(&image_path, dest_ip, port) {
            eprintln!("Failed to send image: {}", e);
        }
    });

    // Spawn the receiver thread
    let receiver_thread = thread::spawn(move || {
        if let Err(e) = receiver::listen_for_image(port, output_path) {
            eprintln!("Failed to receive image: {}", e);
        }
    });

    // Wait for both threads to finish
    sender_thread.join().expect("Sender thread panicked");
    receiver_thread.join().expect("Receiver thread panicked");
}
