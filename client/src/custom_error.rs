use std::error;
use std::error::Error as _;
use std::num::ParseIntError;
use std::fmt;
use shared::deps::tokio as tokio;
use crossterm::ErrorKind;
use tokio::task;
use std::fmt::Formatter;
use shared::deps::thiserror as thiserror;
use thiserror::Error;
use shared::deps::tokio::sync::broadcast::error::RecvError;


pub type Result<T> = std::result::Result<T, Error>;


#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to establish connection to server")]
    Connection(#[source] std::io::Error),
    #[error("Terminal initialization failed")]
    TerminalInitialization(#[source] std::io::Error),
    #[error("Failed to draw frame")]
    DrawFrame(#[source] std::io::Error),
    #[error("Failed to read control event")]
    Control(#[from] crossterm::ErrorKind),
    #[error("Control event listening task failed to complete")]
    EventListeningTaskIncomplete(#[source] shared::error_fill::JoinError),
    #[error("Message sending task failed to complete")]
    MessageSendingTaskIncomplete(#[source] shared::error_fill::JoinError),
    #[error("Drawing sending task failed to complete")]
    DrawTaskIncomplete(#[source] shared::error_fill::JoinError),
}