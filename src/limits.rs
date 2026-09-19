//! Bounds applied to untrusted input.

const MIB: usize = 1024 * 1024;

/// Upper bounds enforced while reading. Every reader has a `_with` variant
/// taking these; the plain variants use [`Limits::default`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    /// Largest accepted file, in bytes. Default 64 MiB.
    pub max_input_bytes: usize,
    /// Largest accepted inflated payload; for zip containers, the total of all
    /// extracted entries. Default 256 MiB.
    pub max_inflated_bytes: usize,
    /// Most entries read from a zip container. Default 4096.
    pub max_archive_entries: usize,
    /// Most images, sprites, frames, shapes and audios decoded into a
    /// [`Movie`](crate::Movie). A frame takes far more memory decoded than
    /// encoded, so this bounds the typed view. Default 4 million.
    pub max_elements: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_input_bytes: 64 * MIB,
            max_inflated_bytes: 256 * MIB,
            max_archive_entries: 4096,
            max_elements: 4_000_000,
        }
    }
}

impl Limits {
    #[must_use]
    pub fn with_max_input_bytes(self, max_input_bytes: usize) -> Self {
        Self {
            max_input_bytes,
            ..self
        }
    }

    #[must_use]
    pub fn with_max_inflated_bytes(self, max_inflated_bytes: usize) -> Self {
        Self {
            max_inflated_bytes,
            ..self
        }
    }

    #[must_use]
    pub fn with_max_archive_entries(self, max_archive_entries: usize) -> Self {
        Self {
            max_archive_entries,
            ..self
        }
    }

    #[must_use]
    pub fn with_max_elements(self, max_elements: usize) -> Self {
        Self {
            max_elements,
            ..self
        }
    }
}
