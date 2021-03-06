



use shared::dep::thiserror;
use thiserror::Error;

use shared::dep::serde_cbor;

pub type Result<T> = std::result::Result<T, Error>;


#[derive(Error, Debug)]
pub enum Error {
    #[error("Can not deserialize client's message as CBOR")]
    DeserializationError(#[from] serde_cbor::Error),
    #[error("The client leaves abruptly")]
    AbruptClientLeave(#[source] std::io::Error),
    #[error("Failed to send pong")]
    SendPong(#[source] std::io::Error),
    #[error("Failed to bind and start server")]
    Bind(#[source] std::io::Error),
    #[error("Failed to accept a connection")]
    Accept(#[source] std::io::Error)
}

pub fn ok()->Result<()>{
    Ok(())
}