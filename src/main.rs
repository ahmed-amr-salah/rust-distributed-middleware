// declare the modules
mod sender;
mod receiver;

// use the required libraries
use std::io;
use std::thread;

fn main() {
    println!("+++++++ Rust Distributed Middleware Client +++++++");
    println!("+++++++ Enter the image path to send over UDP +++++++");
    
    let port = 8080;
    let output_path = "received_image.png";  // Path to save the received image
        
    if let Err(e) = receiver::listen_for_image(port, output_path) {
            eprintln!("Failed to receive image: {}", e);
        }


    }
