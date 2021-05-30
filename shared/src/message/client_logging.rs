


use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct MessageToLogProcess {
    pub level: Level,
    pub formatted_event: String,
}


/// Describes the level of verbosity of a span or event.
#[derive(Debug, Serialize, Deserialize)]
pub enum Level {
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

impl From<tracing_core::Level> for Level {
    fn from(l: tracing_core::Level) -> Self {
        match l {
            tracing_core::Level::TRACE => Level::Trace,
            tracing_core::Level::DEBUG => Level::Debug,
            tracing_core::Level::INFO => Level::Info,
            tracing_core::Level::WARN => Level::Warn,
            tracing_core::Level::ERROR => Level::Error,
        }
    }
}
