use std::{error, fmt};
use std::error::Error as _;
use std::string::FromUtf8Error;
use tokio_util::codec::AnyDelimiterCodecError;


pub type Result<T> = std::result::Result<T, Error>;


#[derive(Debug)]
pub enum Error {
    AbruptClientLeave,
    BadMessageContent(FromUtf8Error),
    BadMessageFormat(AnyDelimiterCodecError),

}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::AbruptClientLeave =>
                write!(f, "The client left abruptly"),
            Error::BadMessageContent(..) => write!(f, "The message from client is not parsable as utf-8 string"),
            Error::BadMessageFormat(..) => write!(f, "The decoder is not able to recognize the message format"),
        }
    }
}

impl From<FromUtf8Error> for Error{
    fn from(err: FromUtf8Error) -> Self {
        Error::BadMessageContent(err)
    }
}
impl From<AnyDelimiterCodecError> for Error{
    fn from(err: AnyDelimiterCodecError) -> Self {
        Error::BadMessageFormat(err)
    }
}


impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            Error::BadMessageContent(ref e) => Some(e),
            Error::BadMessageFormat(ref e) => Some(e),
            _ => None
        }
    }
}