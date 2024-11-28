use std::io::{self, Write};
use std::path::Path;
use tokio::net::UdpSocket;
use serde_json::json;
use uuid::Uuid; 
mod authentication;
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

    // Authentication menu
    println!("Welcome! Please choose an option:");
    println!("1. Register");
    println!("2. Sign In");
    print!("Enter your choice: ");
    io::stdout().flush()?;
    
    let mut choice = String::new();
    io::stdin().read_line(&mut choice)?;
    let choice = choice.trim();

    match choice {
        "1" => {
            // Registration
            let auth_json = authentication::create_auth_json(None);
            let response = authentication::send_auth_request(
                &socket,
                &auth_json,
                &config.server_ips[0],
                config.request_port,
            )
            .await?;

            // Handle server response for registration
            println!("Registration response: {}", response);
            if response.contains("user_id") {
                let parsed_response: serde_json::Value = serde_json::from_str(&response)?;
                if let Some(user_id) = parsed_response["user_id"].as_str() {
                    authentication::save_user_id(user_id, Path::new("user.json"))?;
                } else {
                    eprintln!("Error: Failed to retrieve user_id from response.");
                }
            }
            
        }
        "2" => {
            // Sign In
            print!("Enter your user ID: ");
            io::stdout().flush()?;
            let mut user_id = String::new();
            io::stdin().read_line(&mut user_id)?;
            let user_id = user_id.trim();

            let auth_json = authentication::create_auth_json(Some(user_id));
            let response = authentication::send_auth_request(
                &socket,
                &auth_json,
                &config.server_ips[0],
                config.request_port,
            )
            .await?;

            println!("Sign-in response: {}", response);

            if response.contains("success") {
                // Main menu after successful sign-in
                loop {
                    println!("\nMain Menu:");
                    println!("1. Send Encoded Image");
                    println!("2. Shutdown");
                    print!("Enter your choice: ");
                    io::stdout().flush()?;
                    let mut menu_choice = String::new();
                    io::stdin().read_line(&mut menu_choice)?;
                    let menu_choice = menu_choice.trim();

                    match menu_choice {
                        "1" => {
                            // Send encoded image
                            print!("Enter the path to the image: ");
                            io::stdout().flush()?;
                            let mut input = String::new();
                            io::stdin().read_line(&mut input)?;
                            let image_path = Path::new(input.trim());

                            if !image_path.exists() {
                                eprintln!("The specified image path does not exist.");
                                continue;
                            }
                            if !image_path.is_file() {
                                eprintln!("The specified path is not a file.");
                                continue;
                            }

                            let resource_id = Uuid::new_v4().to_string();
                            workflow::process_image(
                                &socket,
                                image_path,
                                &config.server_ips[0],
                                config.request_port,
                                &resource_id,
                                &config.save_dir,
                            )
                            .await?;
                        }
                        "2" => {
                            // Shutdown
                            let shutdown_json = json!({
                                "type": "shutdown",
                                "user_id": user_id
                            });
                            let response = authentication::send_auth_request(
                                &socket,
                                &shutdown_json,
                                &config.server_ips[0],
                                config.request_port,
                            )
                            .await?;

                            println!("Shutdown response: {}", response);
                            break;
                        }
                        _ => {
                            println!("Invalid choice. Please try again.");
                        }
                    }
                }
            }
        }
        _ => {
            println!("Invalid choice. Exiting.");
        }
    }

    Ok(())
}
