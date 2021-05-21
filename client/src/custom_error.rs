use std::error;
use std::error::Error as _;
use std::num::ParseIntError;
use std::fmt;
use shared::deps::tokio as tokio;
use crossterm::ErrorKind;
use tokio::task;
use std::fmt::Formatter;

pub type Result<T> = std::result::Result<T, Error>;



#[derive(Debug)]
pub enum Error {
    Connection(std::io::Error),
    Control(crossterm::ErrorKind),
    EventListeningTaskIncomplete(shared::error_fill::JoinError),
    MessageSendingTaskIncomplete(shared::error_fill::JoinError),
}


impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {

        match *self {
            Error::Connection(..) =>
                write!(f, "Failed to establish connection to server"),
            // The wrapped error contains additional information and is available
            // via the source() method.
            Error::Control(..) =>
                write!(f, "Failed to read control event"),
            Error::EventListeningTaskIncomplete(..) =>
                write!(f, "Control event listening task failed to complete"),
            Error::MessageSendingTaskIncomplete(..) =>
                write!(f, "Message sending task failed to complete"),

        }
    }
}


impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::Connection(err)
    }
}


impl From<crossterm::ErrorKind> for Error {
    fn from(err: crossterm::ErrorKind) -> Error {
        Error::Control(err)
    }
}




impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            Error::Connection(ref e) => Some(e),
            Error::Control(ref e) => Some(e),
            Error::EventListeningTaskIncomplete(ref e) => Some(e),
            Error::MessageSendingTaskIncomplete(ref e) => Some(e),
        }
    }
}