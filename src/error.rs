//! Errors carry a coarse [`ErrorKind`] plus a stable machine-readable code.
use std::fmt;

/// What went wrong, at the level a caller usually branches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The bytes are not a well-formed SVGA file.
    Malformed,
    /// A valid file that uses something this crate (or this build) cannot read.
    Unsupported,
    /// A configured [`Limits`](crate::Limits) bound was exceeded.
    LimitExceeded,
    /// An edit that cannot be applied, such as replacing a missing image key.
    InvalidEdit,
    /// Compression failed while encoding.
    Encode,
}

/// The error type of this crate.
///
/// [`Error::code`] is a stable identifier such as `svga_truncated_varint`;
/// match on it (or on [`Error::kind`]) rather than on the display text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, thiserror::Error)]
pub struct Error {
    kind: ErrorKind,
    code: &'static str,
}

impl Error {
    pub(crate) const fn malformed(code: &'static str) -> Self {
        Self::new(ErrorKind::Malformed, code)
    }

    pub(crate) const fn unsupported(code: &'static str) -> Self {
        Self::new(ErrorKind::Unsupported, code)
    }

    pub(crate) const fn limit(code: &'static str) -> Self {
        Self::new(ErrorKind::LimitExceeded, code)
    }

    pub(crate) const fn invalid_edit(code: &'static str) -> Self {
        Self::new(ErrorKind::InvalidEdit, code)
    }

    pub(crate) const fn new(kind: ErrorKind, code: &'static str) -> Self {
        Self { kind, code }
    }

    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Stable machine-readable reason, always prefixed with `svga_`.
    pub const fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
