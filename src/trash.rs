use serde::{Serialize, Deserialize};
use steganography::util::file_as_dynamic_image;
use steganography::encoder::Encoder;
use steganography::util::save_image_buffer;
use serde_json::to_vec;
use serde_json::from_slice;

#[derive(Serialize, Deserialize, Debug)]
struct UserQuota {
    username: String,
    quota: i32,
}

fn encode_message() {
    // Example: Create a Vec of UserQuota structs with users and their view quotas
    let users = vec![
        UserQuota { username: "user1".to_string(), quota: 5 },
        UserQuota { username: "user2".to_string(), quota: 3 },
        UserQuota { username: "user3".to_string(), quota: 10 },
    ];

    // Serialize the Vec to a JSON byte array
    let payload = to_vec(&users).unwrap();

    // Load the image where we want to embed our secret message (make sure this file exists)
    let destination_image = file_as_dynamic_image("example.jpeg".to_string());

    // Create an encoder to embed the message into the image (use &payload)
    let enc = Encoder::new(&payload, destination_image);

    // Encode the message into the alpha channel of the image
    let result = enc.encode_alpha();

    // Save the new image with the hidden message
    save_image_buffer(result, "hidden_message.png".to_string());

    println!("Image saved with hidden message.");
}

fn decode_message() {
    // Load the image that contains the hidden message
    let encoded_image = file_as_image_buffer("hidden_message.png".to_string()).unwrap();

    // Create a decoder to extract the hidden message from the image
    let dec = Decoder::new(encoded_image);

    // Decode the image by reading the alpha channel
    let out_buffer = dec.decode_alpha();

    // Deserialize the byte buffer back into a Vec<UserQuota>
    let decoded_users: Vec<UserQuota> = from_slice(&out_buffer).unwrap();

    // Now you can access the data directly in the order it was inserted
    for user_quota in decoded_users {
        println!("Username: {}, View Quota: {}", user_quota.username, user_quota.quota);
    }
}

fn main() {
    encode_message();  // Encode the message
    decode_message();  // Decode and print the message
}


#[derive(Serialize, Deserialize, Debug)]
struct UserQuota {
    username: String,
    quota: i32,
}
