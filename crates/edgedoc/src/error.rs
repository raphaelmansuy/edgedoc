use edgedoc_limit::LimitError;
use edgedoc_merge::MergeError;
use edgedoc_model::ModelError;
use edgedoc_opc::OpcError;
use edgedoc_wml::WmlError;

/// Stable error. Adapters copy `code` and `message`. They do not match on the Rust type.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct Error {
    pub code: &'static str,
    pub message: String,
}

impl Error {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl From<ModelError> for Error {
    fn from(err: ModelError) -> Self {
        Self {
            code: err.code,
            message: err.message,
        }
    }
}

impl From<OpcError> for Error {
    fn from(err: OpcError) -> Self {
        Self {
            code: err.code,
            message: err.message,
        }
    }
}

impl From<WmlError> for Error {
    fn from(err: WmlError) -> Self {
        Self {
            code: err.code,
            message: err.message,
        }
    }
}

impl From<MergeError> for Error {
    fn from(err: MergeError) -> Self {
        Self {
            code: err.code,
            message: err.message,
        }
    }
}

impl From<LimitError> for Error {
    fn from(err: LimitError) -> Self {
        Self {
            code: err.code,
            message: err.message,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self {
            code: "io.destination",
            message: err.to_string(),
        }
    }
}
