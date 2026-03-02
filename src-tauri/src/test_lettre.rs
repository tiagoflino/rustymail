use lettre::message::header::{InReplyTo, References};
use lettre::message::Message;

fn main() {
    let mut builder = Message::builder();
    builder = builder.header(InReplyTo::from("some-id".to_string()));
    builder = builder.header(References::from("some-id".to_string()));
}
