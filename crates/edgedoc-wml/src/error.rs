use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WmlError {
    pub code: &'static str,
    pub message: String,
}

impl WmlError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for WmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for WmlError {}

pub type WmlResult<T> = Result<T, WmlError>;
