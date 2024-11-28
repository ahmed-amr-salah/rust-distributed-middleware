use std::env;
use std::path::PathBuf;
use dotenv::dotenv;

pub struct Config {
    pub save_dir: PathBuf,
    pub request_port: u16,
    pub server_ips: Vec<String>,
}

/// Loads configuration from the environment variables.
pub fn load_config() -> Config {
    dotenv().ok();

    let save_dir = PathBuf::from(env::var("SAVE_DIR").expect("SAVE_DIR is not set in .env"));
    let request_port = env::var("LISTENING_PORT")
        .expect("LISTENING_PORT is not set in .env")
        .parse::<u16>()
        .expect("LISTENING_PORT must be a valid number");

    let server_ips = vec![
        env::var("FIRST_SERVER_IP").expect("FIRST_SERVER_IP is not set in .env"),
        env::var("SECOND_SERVER_IP").expect("SECOND_SERVER_IP is not set in .env"),
    ];

    Config {
        save_dir,
        request_port,
        server_ips,
    }
}
