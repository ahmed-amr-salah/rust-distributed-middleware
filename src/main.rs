use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};
use serde_json::{json, Value, Map};
use uuid::Uuid;
use std::fs;
use std::fs::File;
use std::collections::VecDeque;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rand::rngs::ThreadRng;
use tempfile::TempDir;
mod open_image; // Add this at the top of main.rs if used
use open_image::open_image_with_default_viewer; // Unused in the given code
use tokio::time::Duration;
use tokio::time::sleep;
use tokio::task; // Import for task handling
use futures::future; // Import for joining multiple futures

mod authentication;
mod communication;
mod decode;
mod encode;
mod config;
mod workflow;
mod p2p;

#[tokio::main]
async fn main() -> io::Result<()> {
    // Load environment variables and configuration
    let mut rng = rand::thread_rng(); // This automatically uses a dynamic seed

    // User input: image path and resource ID
    // let mut input = String::new();
    // print!("Enter the folder path: ");
    // io::stdout().flush()?;  // Flush the output properly in async context
    // io::stdin().read_line(&mut input)?; // Read input asynchronously
    let input = "/home/group05-f24/Desktop/Amr's Work/rust-distributed-middleware/images"; // Hardcoded path
    let folder_path = Path::new(input.trim());

    // Validate the folder path
    if !folder_path.exists() {
        eprintln!("The specified folder path does not exist.");
        return Ok(());
    } else if !folder_path.is_dir() {
        eprintln!("The specified path is not a directory.");
        return Ok(());
    }

    // Read all files in the folder
    let image_files: Vec<PathBuf> = fs::read_dir(folder_path)
        .unwrap_or_else(|_| {
            eprintln!("Failed to read the folder.");
            std::process::exit(1);
        })
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_file() {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    if image_files.is_empty() {
        eprintln!("No image files found in the specified folder.");
        return Ok(());
    }

    let mut handles: Vec<tokio::task::JoinHandle<()>> = vec![]; // To collect all the task handles

    // Process each image
    for image_path in image_files {
        // ----> To add time differences between each image sent <---- //
        // sleep(Duration::from_secs(5)).await;

        // Process each image in a non-blocking manner
        let handle = tokio::spawn(async move {
            // let random_num: i32 = rng.gen();
            let socket = match UdpSocket::bind("0.0.0.0:0").await {
                Ok(socket) => socket,
                Err(e) => {
                    eprintln!("Failed to bind socket: {}", e);
                    return; // Exit early if binding fails
                }
            };

            // Extract the image file name (without extension) as the resource ID
            let resource_name = image_path.file_stem()
                .and_then(|os_str| os_str.to_str()) // Convert OsStr to str
                .unwrap_or("unknown") // Fallback if file_stem is invalid
                .to_string();

            let user_id: String = match fs::read_to_string("../user.json") {
                Ok(contents) => {
                    let json: serde_json::Value = serde_json::from_str(&contents).unwrap_or_default();
                    json.get("user_id")
                        .and_then(|id| id.as_str()) // Convert to &str
                        .map(|id| id.to_string()) // Convert to String
                        .unwrap_or_else(|| "0".to_string()) // Fallback to "0" as a String
                }            // let socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?); // Wrap in Arc
            // let socket = Arc::new(Mutex::new(UdpSocket::bind("0.0.0.0:0").await));
            // let socket_ref = socket.lock().await; // Lock the Mutex
                Err(_) => {
                    eprintln!("Failed to read user.json or user_id not found. Using default user_id = 0.");
                    "0".to_string() // Fallback to "0" as a String
                }
            };

            // Create the concatenated resource ID
            let resource_id = format!("client{}-{}", user_id, resource_name);

            // Craft welcome message to send
            let welcome_message = format!("{},{},{}", user_id, resource_id, 12); 

            println!("Concatenated Resource ID: {}", resource_id);

            // Find the server handling the request
            let config = config::load_config();
            let server_ips = &config.server_ips;
            let request_port = config.request_port;
            let save_dir = &config.save_dir;
            if let Ok(Some((server_addr, port))) = workflow::find_server_for_resource(
                &socket,
                server_ips,
                request_port,
                &welcome_message,
            )
            .await
            {
                println!(
                    "Server {}:{} will handle the request for resource ID {}",
                    server_addr, port, resource_id
                );

                // Process the single image
                if let Err(e) = workflow::process_image(
                    &socket,
                    &image_path,
                    &server_addr,
                    port,
                    &resource_id,
                    save_dir,
                )
                .await
                {
                    eprintln!("Error processing image: {}", e);
                }
            } else {
                eprintln!("No server responded to the resource ID. Exiting.");
            }

        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    future::join_all(handles).await;

    Ok(())
}
