use std::io::Write;

use ansi_term;
use futures::StreamExt;
use tokio_util::codec::{FramedRead};


use client::log::protocol::MessageToLogProcess;

use shared::dep::futures as futures;

use shared::dep::serde_cbor as serde_cbor;
use shared::dep::tokio as tokio;
use shared::dep::tokio_util as tokio_util;
use shared::dep::tokio_util::codec::{LengthDelimitedCodec};
use std::fs::File;

// todo: use tcp instead of pipes
#[tokio::main]
async fn main() {
    let mut file = File::create("log.txt").unwrap();

    ansi_term::enable_ansi_support().unwrap();

    let mut framed_read = FramedRead::new(tokio::io::stdin(),
                                          LengthDelimitedCodec::new());

    async move {
        while let Some(maybe_bytes) = framed_read.next().await {
            match maybe_bytes {
                Ok(received_bytes) => {
                    match serde_cbor::
                    from_slice::
                    <MessageToLogProcess>(received_bytes.as_ref()) {
                        Ok(message) => {
                            match std::io::stdout()
                                .write_all(message.formatted_event.as_ref()) {
                                Ok(_) => {}
                                Err(e) => {
                                    writeln!(file, "Failed to write to stdout: {:?}", e).unwrap();
                                }
                            }
                        }
                        Err(e) => {
                            writeln!(file, "Failed to deserialize message: {:?}", e).unwrap();
                        }
                    }
                }
                Err(e) => {
                    writeln!(file, "Failed to receive bytes: {:?}", e).unwrap();
                }
            }
        }
        writeln!(file, "Exited").unwrap();
    }.await;


    ()
}