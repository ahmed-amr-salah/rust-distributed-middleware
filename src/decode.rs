use steganography::util::file_as_image_buffer;
use steganography::decoder::Decoder;
use steganography::util::bytes_to_str;


pub fn decode_message(){
    // Load the image that contains the hidden message
    // No need to unwrap, because it's not a Result type
    let encoded_image = file_as_image_buffer("hidden_message.png".to_string());

    // Create a decoder to extract the hidden message from the image
    let dec = Decoder::new(encoded_image);

    // Decode the image by reading the alpha channel
    let out_buffer = dec.decode_alpha();

    // Filter out any padding or default values (255 is used as padding)
    let clean_buffer: Vec<u8> = out_buffer.into_iter()
        .filter(|b| *b != 0xff_u8)
        .collect();

    // Convert the byte buffer back into a string
    let message = bytes_to_str(clean_buffer.as_slice());

    // Print out the hidden message
    println!("Decoded message: {:?}", message);
}