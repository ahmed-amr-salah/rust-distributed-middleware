// // use ferris_says::say; // from the previous step
// // use std::io::{stdout, BufWriter};
// fn main() {
//     // let stdout = stdout();
//     // let message = String::from("Hello fellow Rustaceans!");
//     // let width = message.chars().count();

//     // let mut writer = BufWriter::new(stdout.lock());
//     // say(&message, width, &mut writer).unwrap();

//     // let mario = String::from("yarb");
//     // say(&mario, width, &mut writer).unwrap();

//     println!("Hello Crab!");
// }
///////////////////////////////

extern crate steganography;
// mod encode;
// mod decode;
mod encode2;
mod decode2;

fn main(){
    // encode::encode_message();
    
    // decode::decode_message();

    encode2::encode_message();

    decode2::decode_message();
}