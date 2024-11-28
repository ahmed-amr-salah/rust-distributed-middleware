use std::io::{self, Write};
use std::path::Path;
use tokio::net::UdpSocket;
use uuid::Uuid; 
mod communication;
mod decode;
mod encode;
mod config;
mod workflow;

#[tokio::main]
async fn main() -> io::Result<()> {
    // Load environment variables and configuration
    let config = config::load_config();
    let socket = UdpSocket::bind("0.0.0.0:0").await?;

    // User input: image path and resource ID
    let mut input = String::new();
    print!("Enter the path to the image: ");
    io::stdout().flush()?;
    io::stdin().read_line(&mut input)?;
    let image_path = Path::new(input.trim());
    
    // Validate the image path
    if !image_path.exists() {
        eprintln!("The specified image path does not exist.");
        return Ok(());
    }
    if !image_path.is_file() {
        eprintln!("The specified path is not a file.");
        return Ok(());
    }
    ///////////////////////////////////////////////////////
    // Generate a random resource ID
    let resource_id = Uuid::new_v4().to_string();
    println!("Generated unique resource ID: {}", resource_id);
    ///////////////////////////////////////////////////////

    // Find the server handling the request
    if let Some((server_addr, port)) =
        workflow::find_server_for_resource(&socket, &config.server_ips, config.request_port, &resource_id).await?
    {
        println!("Server {}:{} will handle the request for resource ID {}", server_addr, port, resource_id);

        // Process the single image
        workflow::process_image(
            &socket,
            image_path,
            &server_addr,
            port,
            &resource_id,
            &config.save_dir,
        )
        .await?;
    } else {
        eprintln!("No server responded to the resource ID. Exiting.");
    }

    Ok(())
}
