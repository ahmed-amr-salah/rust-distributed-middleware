use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};
use serde_json::{json, Value, Map};
use uuid::Uuid;
use std::fs::{self, OpenOptions};
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
use std::time::Instant;
use tokio::sync::Semaphore;

mod authentication;
mod communication;
mod decode;
mod encode;
mod config;
mod workflow;
mod p2p;


#[tokio::main]
async fn main() -> io::Result<()> {
    const NUM_ITERATIONS: usize = 100;
    const MAX_CONCURRENT_TASKS: usize = 10; // Limit concurrency to avoid stack overflow

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_TASKS));

    let input = "/home/group05-f24/Desktop/Amr's Work/rust-distributed-middleware/images";
    let folder_path = Path::new(input.trim());
    let times_file_path = folder_path.parent().unwrap().join("times.txt");

    if !folder_path.exists() || !folder_path.is_dir() {
        eprintln!("The specified path is invalid or not a directory.");
        return Ok(());
    }

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

    let mut handles = vec![];

    for iteration in 0..NUM_ITERATIONS {
        for image_path in &image_files {
            let image_path = image_path.clone();
            let times_file_path = times_file_path.clone();
            let semaphore = semaphore.clone();

            let handle = tokio::spawn(async move {
                // Acquire a permit from the semaphore
                let _permit = semaphore.acquire().await.unwrap();

                let start_time = Instant::now();

                let socket = match UdpSocket::bind("0.0.0.0:0").await {
                    Ok(socket) => socket,
                    Err(e) => {
                        eprintln!("Failed to bind socket: {}", e);
                        return;
                    }
                };

                let resource_name = image_path.file_stem()
                    .and_then(|os_str| os_str.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let user_id: String = match fs::read_to_string("../user.json") {
                    Ok(contents) => {
                        let json: serde_json::Value = serde_json::from_str(&contents).unwrap_or_default();
                        json.get("user_id")
                            .and_then(|id| id.as_str())
                            .map(|id| id.to_string())
                            .unwrap_or_else(|| "0".to_string())
                    }
                    Err(_) => {
                        eprintln!("Failed to read user.json or user_id not found. Using default user_id = 0.");
                        "0".to_string()
                    }
                };

                let resource_id = format!("client{}-{}-iter{}", user_id, resource_name, iteration);

                let welcome_message = format!("{},{},{}", user_id, resource_id, 12);

                println!("Processing resource ID: {}", resource_id);

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

                let elapsed_time = start_time.elapsed();

                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(times_file_path)
                    .expect("Failed to open or create times.txt");

                writeln!(file, "{} ms", elapsed_time.as_millis())
                    .expect("Failed to write to times.txt");
            });
            handles.push(handle);
        }
    }

    future::join_all(handles).await;

    Ok(())
}
