use std::fmt;


use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct MessageToLogProcess {
    pub level: Level,
    pub formatted_event: String,
}


/// Describes the level of verbosity of a span or event.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Level(LevelInner);

#[repr(usize)]
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum LevelInner {
    /// The "trace" level.
    ///
    /// Designates very low priority, often extremely verbose, information.
    Trace = 0,
    /// The "debug" level.
    ///
    /// Designates lower priority information.
    Debug = 1,
    /// The "info" level.
    ///
    /// Designates useful information.
    Info = 2,
    /// The "warn" level.
    ///
    /// Designates hazardous situations.
    Warn = 3,
    /// The "error" level.
    ///
    /// Designates very serious errors.
    Error = 4,
}

impl From<tracing_core::Level> for Level{
    fn from(l: tracing_core::Level) -> Self {
        match l {
            tracing_core::Level::TRACE => Level{ 0: LevelInner::Trace },
            tracing_core::Level::DEBUG => Level{ 0: LevelInner::Debug },
            tracing_core::Level::INFO => Level{ 0: LevelInner::Info },
            tracing_core::Level::WARN => Level{ 0: LevelInner::Warn },
            tracing_core::Level::ERROR => Level{ 0: LevelInner::Error },
        }
    }
}



#[cfg(test)]
mod tests {
    use super::*;
}


pub struct EventLevel<'a> {
    pub level: & 'a tracing_core::Level,
}




impl<'a> fmt::Display for EventLevel<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self.level {
            tracing_core::Level::TRACE => write!(f,"{}", TRACE_STR),
            tracing_core::Level::DEBUG => write!(f,"{}", DEBUG_STR),
            tracing_core::Level::INFO => write!(f,"{}", INFO_STR),
            tracing_core::Level::WARN => write!(f,"{}", WARN_STR),
            tracing_core::Level::ERROR => write!(f, "{}",ERROR_STR),
        }
    }
}

const TRACE_STR: &str = "TRACE";
const DEBUG_STR: &str = "DEBUG";
const INFO_STR: &str = " INFO";
const WARN_STR: &str = " WARN";
const ERROR_STR: &str = "ERROR";
