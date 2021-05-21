use std::error;
use std::error::Error as _;
use std::num::ParseIntError;
use std::fmt;
use crossterm::ErrorKind;
use tokio::task;
use std::fmt::Formatter;

pub type Result<T> = std::result::Result<T, ClientError>;



#[derive(Debug)]
pub enum ClientError {
    Connection(std::io::Error),
    Control(crossterm::ErrorKind),
    EventListeningTaskIncomplete(crate::error_fill::JoinError),
    MessageSendingTaskIncomplete(crate::error_fill::JoinError),
}


impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {

        match *self {
            ClientError::Connection(..) =>
                write!(f, "Failed to establish connection to server"),
            // The wrapped error contains additional information and is available
            // via the source() method.
            ClientError::Control(..) =>
                write!(f, "Failed to read control event"),
            ClientError::EventListeningTaskIncomplete(..) =>
                write!(f, "Control event listening task failed to complete"),
            ClientError::MessageSendingTaskIncomplete(..) =>
                write!(f, "Message sending task failed to complete"),

        }
    }
}


impl From<std::io::Error> for ClientError {
    fn from(err: std::io::Error) -> ClientError {
        ClientError::Connection(err)
    }
}


impl From<crossterm::ErrorKind> for ClientError {
    fn from(err: crossterm::ErrorKind) -> ClientError {
        ClientError::Control(err)
    }
}




impl error::Error for ClientError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            ClientError::Connection(ref e) => Some(e),
            ClientError::Control(ref e) => Some(e),
            ClientError::EventListeningTaskIncomplete(ref e) => Some(e),
            ClientError::MessageSendingTaskIncomplete(ref e) => Some(e),
        }
    }
}