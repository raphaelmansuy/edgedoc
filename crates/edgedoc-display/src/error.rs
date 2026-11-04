//! The facade error type.
//!
//! Every code is stable, and a caller matches on the code, not on the message. See `DP-V-08` and
//! `DP-E-01`.

use std::fmt;

/// A failure that stops the document from opening.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// A stable code, such as `package.not_word` or `display.flat_opc_unsupported`.
    pub code: String,
    pub message: String,
}

impl Error {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    /// True when the package is encrypted, which needs a password.
    pub fn is_password(&self) -> bool {
        self.code == "package.password_required"
    }

    /// The next action that the message names, for a viewer to show.
    pub fn action(&self) -> &'static str {
        match self.code.as_str() {
            "package.password_required" => "Enter password",
            "package.not_zip" | "package.not_word" | "package.missing_part" => {
                "Choose another file"
            }
            "package.limit" | "display.page_limit" | "display.memory_limit" => {
                "Choose a smaller file"
            }
            _ => "Choose another file",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}

/// A result from the facade.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_renders_its_code_and_message() {
        let e = Error::new("package.not_word", "The package is not a Word document.");
        assert_eq!(
            e.to_string(),
            "package.not_word The package is not a Word document."
        );
    }

    #[test]
    fn a_password_error_is_recognised() {
        assert!(Error::new("package.password_required", "").is_password());
        assert!(!Error::new("package.not_word", "").is_password());
    }

    #[test]
    fn an_error_names_the_next_action() {
        assert_eq!(
            Error::new("package.password_required", "").action(),
            "Enter password"
        );
        assert_eq!(
            Error::new("package.not_zip", "").action(),
            "Choose another file"
        );
    }
}
