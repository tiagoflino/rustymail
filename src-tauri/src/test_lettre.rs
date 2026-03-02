use lettre::message::{header::ContentType, Mailbox, Message};
use std::str::FromStr;

fn main() {
    let from = "tiago.fortunato@gmail.com";
    let from_mailbox = Mailbox::from_str(from).unwrap();

    let builder = Message::builder()
        .from(from_mailbox)
        .subject("Test")
        .header(ContentType::TEXT_HTML);

    let res = builder.body("Hello World".to_string());

    match res {
        Ok(_) => println!("Success!"),
        Err(e) => println!("Error: {}", e),
    }
}
