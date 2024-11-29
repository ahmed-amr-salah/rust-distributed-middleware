use serde_json::{json, Value};
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use tokio::net::UdpSocket;



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
