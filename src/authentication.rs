use serde_json::{json, Value};
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use tokio::net::UdpSocket;

/// Creates a JSON object for user registration or sign-in.
///
/// # Arguments
/// - `user_id`: The user's unique ID. Pass `None` for registration.
///
/// # Returns
/// A `Value` representing the JSON object.
pub fn create_auth_json(user_id: Option<&str>) -> Value {
    match user_id {
        Some(id) => json!({
            "type": "sign_in",
            "user_id": id
        }),
        None => json!({
            "type": "register"
        }),
    }
}


/// Sends a JSON object to the server and waits for a response.
///
/// # Arguments
/// - `socket`: A reference to the bound `UdpSocket`.
/// - `auth_json`: The JSON object to send.
/// - `server_ip`: The server's IP address.
/// - `port`: The port to send to.
///
/// # Returns
/// The server's response as a `String`.
pub async fn send_auth_request(
    socket: &UdpSocket,
    auth_json: &Value,
    server_ip: &str,
    port: u16,
) -> io::Result<String> {
    let server_addr = format!("{}:{}", server_ip, port);
    println!("Sending authentication request to {}", server_addr);
    // Serialize the JSON object to a string
    let auth_data = auth_json.to_string();

    // Send the JSON data to the server
    socket.send_to(auth_data.as_bytes(), &server_addr).await?;
    println!("Authentication request sent to {}", server_addr);

    // Wait for the server's response
    let mut buffer = [0u8; 1024];
    let (size, _) = socket.recv_from(&mut buffer).await?;
    let response = String::from_utf8_lossy(&buffer[..size]).to_string();

    Ok(response)
}

/// Saves the received unique user ID to a local JSON file.
///
/// # Arguments
/// - `user_id`: The unique user ID to save.
/// - `path`: Path to the JSON file.
pub fn save_user_id(user_id: &str, path: &Path) -> io::Result<()> {
    let user_data = json!({ "user_id": user_id });
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, &user_data)?;
    println!("User ID saved to {:?}", path);
    Ok(())
}
