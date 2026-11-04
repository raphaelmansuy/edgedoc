use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpcError {
    pub code: &'static str,
    pub message: String,
}

impl OpcError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for OpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for OpcError {}

pub type OpcResult<T> = Result<T, OpcError>;
