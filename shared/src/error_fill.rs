use std::fmt;
use std::fmt::Formatter;


/// tokio's JoinError doesn't impl the std::error::Error trait, 属实垃圾
/// A wrapper here for it to better fit into our supreme error handling system.
#[derive(Debug)]
pub struct JoinError{
    join_error: tokio::task::JoinError
}

impl JoinError{
    fn new(join_error: tokio::task::JoinError) ->Self{
        JoinError{join_error}
    }
}

impl std::error::Error for JoinError{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

impl From<tokio::task::JoinError> for JoinError{
    fn from(join_error: tokio::task::JoinError) -> Self {
        Self{join_error}
    }
}

impl fmt::Display for JoinError{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.join_error.is_cancelled(){
            return write!(f, "Task failed to join, the task was cancelled")
        }
        if self.join_error.is_panic(){
            return write!(f, "Task failed to join, the task panicked")
        }

        return write!(f, "Task failed to join")
    }
}
