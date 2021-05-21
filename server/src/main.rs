use std::str::FromStr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_stream::{Stream, StreamExt};
use tokio_util::codec::{AnyDelimiterCodec, FramedRead, LinesCodec};

use shared::deps::tokio as tokio;
use shared::message::{CLIENT_QUIT_MESSAGE, ClientToServerMessage, MESSAGE_DELIMITER};

use crate::custom_errors::Error;

mod custom_errors;


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    loop {
        println!("Waiting for new connection...");
        let (mut socket, address) = listener.accept().await?;
        println!("Accepted connection from {}", address);
        tokio::spawn(async move {

            let mut framed = shared::message::default_framed(socket);


            while let Some(received_bytes) = framed.next().await{
                let received_bytes = received_bytes?;
                let received_string = String::from_utf8(Vec::from(&received_bytes[..]))?;
                println!("{}", received_string);
                if received_string == CLIENT_QUIT_MESSAGE.to_string() {break}
            }

            println!("Client {} leaves", address);


        Ok::<(), Error>(())
        });
    }
}