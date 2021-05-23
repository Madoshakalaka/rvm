use std::{fmt, io};
use std::error::Error;

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::runtime::Handle;
use tokio::time::{Duration, sleep};

use shared::deps::tokio as tokio;




#[tokio::main]
async fn main() {
    if let Err(e) = client::run().await {
        shared::error_util::eprint_error_chain(e);
        std::process::exit(-1);
    };
}