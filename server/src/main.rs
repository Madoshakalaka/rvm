use shared::dep::bytes as bytes;
use bytes::Bytes;

use tokio::net::TcpListener;
use tokio_stream::{StreamExt};


use shared::dep::tokio as tokio;

use shared::dep::serde_cbor;
use shared::message::{CLIENT_QUIT_MESSAGE, ClientToServerMessage, ServerToClientMessage};

use crate::error::Error;
use shared::dep::futures::{SinkExt, TryFutureExt};
use error::Result;


mod error;


async fn run() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await.map_err(|e| Error::Bind(e))?;

    loop {
        println!("Waiting for new connection...");
        let (socket, address) = listener.accept().await.map_err(|e| Error::Accept(e))?;
        println!("Accepted connection from {}", address);
        tokio::spawn(async move {
            let mut framed = shared::message::default_framed(socket);


            while let Some(received_bytes) = framed.next().await {
                let received_bytes = received_bytes.map_err(|e| Error::AbruptClientLeave(e))?;
                let received = serde_cbor::from_slice::<ClientToServerMessage>(&*received_bytes).map_err(|e| Error::DeserializationError(e))?;

                println!("{:?}", received);
                match received {
                    CLIENT_QUIT_MESSAGE => {
                        break;
                    }
                    ClientToServerMessage::W => {}
                    ClientToServerMessage::A => {}
                    ClientToServerMessage::S => {}
                    ClientToServerMessage::D => {}
                    ClientToServerMessage::J => {}
                    ClientToServerMessage::Spacebar => {}
                    ClientToServerMessage::P => {}
                    ClientToServerMessage::Ping(time) => {
                        println!("Sending Pong back to {}", address);
                        framed.send(Bytes::from(serde_cbor::to_vec(&ServerToClientMessage::Pong(time)).unwrap())).await
                            .map_err(|e| Error::SendPong(e))?
                    }
                }
            }
            println!("Client {} leaves", address);
            error::ok()
        }.map_err(|x| {
            shared::error_util::eprint_error_chain(x);
        })
        );
    }
}


#[tokio::main]
async fn main() -> () {
    if let Err(e) = run().await {
        shared::error_util::eprint_error_chain(e);
        std::process::exit(-1);
    }
}