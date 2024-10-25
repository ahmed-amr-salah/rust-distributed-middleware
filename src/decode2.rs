use serde_json::from_slice;
use std::collections::HashMap;
use steganography::util::file_as_image_buffer;
use steganography::decoder::Decoder;

pub fn decode_message() {
    // Load the image that contains the hidden message
    let encoded_image = file_as_image_buffer("hidden_message.png".to_string());

    // Create a decoder to extract the hidden message from the image
    let dec = Decoder::new(encoded_image);

    // Decode the image by reading the alpha channel
    let out_buffer = dec.decode_alpha();

    // Filter out padding bytes (e.g., 0xFF used in the alpha channel)
    let clean_buffer: Vec<u8> = out_buffer.into_iter()
        .filter(|&b| b != 0xFF) // Filter out 0xFF, which is often padding in images
        .collect();

    // Deserialize the cleaned byte buffer back into a HashMap
    let decoded_users: HashMap<String, i32> = from_slice(&clean_buffer).unwrap();

    // Now you can access the data directly
    for (user, quota) in decoded_users {
        // println!("Username: {}, View Quota: {}", user, quota);
        println!("{} {}", user, quota);
    }
}
