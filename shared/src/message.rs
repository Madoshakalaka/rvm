use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use strum;
use strum::Display;
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

/// separates two messages
pub const MESSAGE_DELIMITER: &[u8; 1] = b"\x19";


pub const CLIENT_QUIT_MESSAGE: ClientToServerMessage = ClientToServerMessage::IWantToDisconnect;


/// ```
/// use shared::message::ClientToServerMessage;
///
/// let encoded_variant = serde_cbor::to_vec(& ClientToServerMessage::IWantToDisconnect).unwrap();
///
/// let parsed_variant: ClientToServerMessage =  serde_cbor::from_slice(& encoded_variant).unwrap();
/// assert_eq!(parsed_variant, ClientToServerMessage::IWantToDisconnect);
///
/// let stringified_variant = ClientToServerMessage::IWantToDisconnect.to_string();
/// assert_eq!(stringified_variant, "IWantToDisconnect");
/// ```
#[derive(Debug, Display, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub enum ClientToServerMessage {
    IWantToDisconnect,
    W,
    A,
    S,
    D,
    J,
    Spacebar,
    P,
    Ping(SystemTime),
}


#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum ServerToClientMessage {
    /// the time is the sender client's system time when it sends the ping
    Pong(SystemTime)
}


// const MESSAGE_BUFF: BytesMut = BytesMut::with_capacity(100);


pub fn default_framed(stream: TcpStream) -> Framed<TcpStream, LengthDelimitedCodec> {
    Framed::new(stream, LengthDelimitedCodec::new())
}


#[cfg(test)]
mod tests {
    use super::*;
}
