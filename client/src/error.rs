



use shared::dep::tokio as tokio;
use tracing::error;

use std::fmt::Formatter;
use shared::dep::thiserror as thiserror;
use thiserror::Error;

use shared::dep::serde_cbor as serde_cbor;
use shared::message::ClientToServerMessage;
use std::num::TryFromIntError;
use std::time::SystemTimeError;


pub type Result<T=()> = std::result::Result<T, Error>;


#[derive(Error, Debug)]
pub enum PingError{
    #[error("ping value overflow")]
    GamerLag(#[from] TryFromIntError),
    #[error("Received PONG from server with future time")]
    TimeTraveler(#[from] SystemTimeError)
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to establish connection to server")]
    Connection(#[source] std::io::Error),
    #[error("Failed to shutdown the TCP stream")]
    TcpStreamShutdown(#[source] std::io::Error),
    #[error("Failed to send message to server")]
    SendMessage(#[source] std::io::Error),
    #[error("Terminal initialization failed")]
    TerminalInitialization(#[source] std::io::Error),
    #[error("Failed to draw frame")]
    DrawFrame(#[source] std::io::Error),
    #[error("Failed to decode server message")]
    DecodeFromServer(#[source] std::io::Error),
    #[error("Failed to read control event")]
    ControlRead(#[from] crossterm::ErrorKind),
    #[error("Failed to send control message")]
    ControlSend(#[from] tokio::sync::broadcast::error::SendError<ClientToServerMessage>),
    #[error("Failed to send debug message")]
    DebugSend(#[from] tokio::sync::mpsc::error::SendError<String>),
    #[error("Failed to clear the terminal")]
    ClearTerminal(#[source] std::io::Error),
    #[error("Main task failed to join")]
    MainTaskJoin(#[from] JoinError),
    #[error("Failed to decode server message as cbor")]
    ServerMessageDecode(#[from] serde_cbor::Error),
    #[error("Holy Spirit failed to format timestamp inside event")]
    FormatTimestamp(#[source] std::fmt::Error),
    #[error("Holy Spirit failed to format thread name")]
    FormatThreadName(#[source] std::fmt::Error),
    #[error("Holy Spirit failed to format full context")]
    FormatFullContext(#[source] std::fmt::Error),
    #[error("Holy Spirit failed to format target")]
    FormatTarget(#[source] std::fmt::Error),
    #[error("Holy Spirit failed to format fields")]
    FormatFields(#[source] std::fmt::Error),

}

pub fn ok()->Result<()>{
    Ok(())
}

/// tokio's JoinError doesn't impl the std::error::Error trait, 属实垃圾
/// A wrapper here for it to better fit into our supreme error handling system.
#[derive(Error, Debug)]
pub struct JoinError{
    wrapped: tokio::task::JoinError
}

impl std::fmt::Display for JoinError{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.wrapped.is_panic(){
            f.write_str("Task panicked")
        }else if self.wrapped.is_cancelled() {
            f.write_str("Task panicked")
        } else{
            f.write_str("Task failed to complete")
        }
    }
}

impl From<tokio::task::JoinError> for JoinError{

    fn from(wrapped: tokio::task::JoinError) -> Self {
        Self{wrapped}
    }
}

pub type SpawnedTaskResult<T=Result> = std::result::Result<T, tokio::task::JoinError>;



pub(crate) fn from_spawned_task_result<T>(task_result: SpawnedTaskResult<Result<T>>) -> Result<T>{
    match task_result {
        Ok(r) => {
            r
        }
        Err(e) => Err(Error::MainTaskJoin(e.into()))
    }
}



fn _report_error_chain(e: impl std::error::Error, is_source:bool){
    if let Some(source) = e.source(){
        _report_error_chain(source, true);
    }
    match is_source{
        true => {
            error!("{}\tThis is the cause of the next error.", e);
        }
        false => {
            error!("{}", e);
        }
    }

}


pub fn report_error_chain(e: impl std::error::Error){
    _report_error_chain(e, false);
}
