use std::{fmt, io};
use std::error::Error;

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::runtime::Handle;
use tokio::time::{Duration, sleep};

use shared::deps::tokio as tokio;




#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    client::run().await?;
    Ok(())
}