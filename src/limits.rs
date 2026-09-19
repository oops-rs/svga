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
    /// Most top-level protobuf fields kept by a
    /// [`Document`](crate::Document). Each costs far more memory than the two
    /// bytes it can be stored in. Default 250 000.
    pub max_fields: usize,
    /// Largest accepted SVGA 1.x `movie.spec`; parsed JSON takes many times
    /// its size. Default 16 MiB.
    pub max_spec_bytes: usize,
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
            max_fields: 250_000,
            max_spec_bytes: 16 * MIB,
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
    pub fn with_max_fields(self, max_fields: usize) -> Self {
        Self { max_fields, ..self }
    }

    #[must_use]
    pub fn with_max_spec_bytes(self, max_spec_bytes: usize) -> Self {
        Self {
            max_spec_bytes,
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
