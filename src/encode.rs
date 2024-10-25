use steganography::util::str_to_bytes;
use steganography::util::file_as_dynamic_image;
use steganography::encoder::Encoder;
use steganography::util::save_image_buffer;

pub fn encode_message(){
    // Example: adding two users with their view quotas
    let users = vec![
        ("user123", 5),
        ("user456", 3),
        ("user789", 10),
    ];

    // Create a string to represent the data we want to hide, e.g., "user123:5,user456:3,user789:10"
    let message: String = users.iter()
        .map(|(username, quota)| format!("{}:{}", username, quota))
        .collect::<Vec<String>>()
        .join(",");  // Separate each user data with a comma

    // Convert the message string to bytes
    let payload = str_to_bytes(&message);

    // Load the image where we want to embed our secret message (make sure this file exists)
    let destination_image = file_as_dynamic_image("example.jpeg".to_string());

    // Create an encoder to embed the message into the image
    let enc = Encoder::new(payload, destination_image);

    // Encode the message into the alpha channel of the image
    let result = enc.encode_alpha();

    // Save the new image with the hidden message
    save_image_buffer(result, "hidden_message.png".to_string());

    println!("Image saved with hidden message: {}", message);
}
