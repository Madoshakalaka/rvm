use std::fmt;
use strum::{EnumString, Display};
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, AnyDelimiterCodec, AnyDelimiterCodecError};

/// the below four allows stream! macro to work
use async_stream::try_stream;
use futures_core::stream::Stream;
use futures_util::pin_mut;
use futures_util::stream::StreamExt;
use bytes::{BytesMut, BufMut, Bytes};

///

pub const MESSAGE_DELIMITER: &[u8; 1] = b"\x19";
pub const CLIENT_QUIT_MESSAGE: ClientToServerMessage = ClientToServerMessage::IWantToDisconnect;


/// ```
/// use shared::message::ClientToServerMessage;
/// use std::str::FromStr;
///
/// let stringified_variant = ClientToServerMessage::IWantToDisconnect.to_string();
/// assert_eq!(stringified_variant, "IWantToDisconnect");
///
/// let parsed_variant: ClientToServerMessage =  ClientToServerMessage::from_str("IWantToDisconnect").unwrap();
/// assert!(matches!(parsed_variant, ClientToServerMessage::IWantToDisconnect));
///
/// ```
#[derive(Debug, EnumString, Display, PartialEq)]
pub enum ClientToServerMessage {
    IWantToDisconnect,
    W,
    A,
    S,
    D,
    J,
    Spacebar,
    P,
}

// const MESSAGE_BUFF: BytesMut = BytesMut::with_capacity(100);

impl ClientToServerMessage {

    /// stream an variant character by character as u8, with pending delimiter
    /// remember to pin_mut! the return value before starts iterating
    pub fn with_pending_delimiter(message: ClientToServerMessage) -> impl Stream<Item=std::result::Result<String, AnyDelimiterCodecError>>{
        let string_message = message.to_string();

        try_stream! {

            yield string_message;

        }
    }
}


pub fn default_framed(stream: TcpStream) -> Framed<TcpStream, AnyDelimiterCodec> {
    Framed::new(stream, AnyDelimiterCodec::new(MESSAGE_DELIMITER.to_vec(), MESSAGE_DELIMITER.to_vec()))
}


#[cfg(test)]
mod tests {

    use super::*;


    #[tokio::main]
    #[test]
    async fn with_pending_delimiter_works() {

        let stream = ClientToServerMessage::with_pending_delimiter
            (ClientToServerMessage::Backspace);
        pin_mut!(stream);
        let answer = [b'B', b'a', b'c', b'k', b's', b'p', b'a', b'c', b'e', MESSAGE_DELIMITER[0]];

        let mut i = 0;

        while let Some(x) = stream.next().await{
            assert_eq!(answer[i], x.unwrap());
            i = i +1;
        }
    }
}
