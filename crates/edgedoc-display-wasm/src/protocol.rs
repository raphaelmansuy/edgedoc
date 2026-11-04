//! The message protocol between the viewer and the worker.
//!
//! A message names a document, a sequence number, and a request. The reply carries the same
//! sequence, so a stale reply is dropped. See `DP-B-07` to `DP-B-13`.

/// The protocol version. A viewer refuses a mismatch. See `DP-B-13`.
pub const PROTOCOL_VERSION: u32 = 1;

/// The largest message that the protocol accepts, in bytes. See `DP-B-12`.
pub const MAX_MESSAGE_BYTES: usize = 16 << 20;

/// A request from the viewer to the worker.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// Start the module and publish its version.
    Init,
    /// Open a package.
    Load { bytes: Vec<u8> },
    /// Lay out and encode every page.
    Layout,
    /// Request one page display list.
    Page { index: u32 },
    /// Search the document.
    Search { query: String },
    /// Hit test a point.
    HitTest { page: u32, x: f32, y: f32 },
    /// Release the document.
    Unload,
}

/// A reply from the worker to the viewer.
#[derive(Debug, Clone, PartialEq)]
pub enum Response {
    Ready {
        protocol: u32,
        version: String,
    },
    Loaded {
        pages: u32,
        outline: String,
        diagnostics: String,
    },
    Page {
        index: u32,
        bytes: Vec<u8>,
        w: f32,
        h: f32,
    },
    LaidOut {
        pages: u32,
        passes: usize,
        elapsed_ms: f64,
    },
    Search {
        json: String,
    },
    Hit {
        json: String,
    },
    Unloaded,
    Error {
        code: String,
        message: String,
    },
}

impl Response {
    /// The message type name, which the viewer switches on.
    pub fn kind(&self) -> &'static str {
        match self {
            Response::Ready { .. } => "ready",
            Response::Loaded { .. } => "loaded",
            Response::Page { .. } => "page",
            Response::LaidOut { .. } => "laidout",
            Response::Search { .. } => "search",
            Response::Hit { .. } => "hit",
            Response::Unloaded => "unloaded",
            Response::Error { .. } => "error",
        }
    }
}

/// A message envelope, which a worker posts and a viewer reads.
#[derive(Debug, Clone, PartialEq)]
pub struct Envelope<T> {
    /// The message type.
    pub kind: String,
    /// The document id.
    pub id: String,
    /// The request sequence.
    pub seq: u32,
    pub payload: T,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_response_has_a_distinct_kind() {
        let all = [
            Response::Ready {
                protocol: 1,
                version: "0.1.0".into(),
            },
            Response::Loaded {
                pages: 1,
                outline: "[]".into(),
                diagnostics: "[]".into(),
            },
            Response::Page {
                index: 0,
                bytes: vec![1],
                w: 1.0,
                h: 1.0,
            },
            Response::LaidOut {
                pages: 1,
                passes: 1,
                elapsed_ms: 1.0,
            },
            Response::Search { json: "{}".into() },
            Response::Hit { json: "{}".into() },
            Response::Unloaded,
            Response::Error {
                code: "x".into(),
                message: "y".into(),
            },
        ];
        let mut kinds: Vec<&str> = all.iter().map(|r| r.kind()).collect();
        let before = kinds.len();
        kinds.sort_unstable();
        kinds.dedup();
        assert_eq!(kinds.len(), before);
    }

    #[test]
    fn the_message_limit_is_sixteen_mib() {
        assert_eq!(MAX_MESSAGE_BYTES, 16 * 1024 * 1024);
    }
}
