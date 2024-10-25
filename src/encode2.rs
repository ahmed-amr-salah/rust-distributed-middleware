use serde_json::to_vec;
use std::collections::HashMap;
use steganography::util::file_as_dynamic_image;
use steganography::encoder::Encoder;
use steganography::util::save_image_buffer;

pub fn encode_message() {
    // Example: Create a HashMap with users and their view quotas
    let mut users = HashMap::new();
    users.insert("user1", 5);
    users.insert("user2", 3);
    users.insert("user3", 10);

    // Serialize the HashMap to a JSON byte array
    let payload = to_vec(&users).unwrap(); // Convert the map into a byte array

    // Load the image where we want to embed our secret message (make sure this file exists)
    let destination_image = file_as_dynamic_image("example.jpeg".to_string());

    // Create an encoder to embed the message into the image
    let enc = Encoder::new(&payload, destination_image);

    // Encode the message into the alpha channel of the image
    let result = enc.encode_alpha();

    // Save the new image with the hidden message
    save_image_buffer(result, "hidden_message.png".to_string());

    println!("Image saved with hidden message.");
}
