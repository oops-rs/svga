//! The lossless layer: a `MovieEntity` kept as its original top-level fields.
//!
//! Nothing is decoded into structs here. Every field keeps its stored bytes,
//! so unknown fields, field order and float bit patterns survive re-encoding
//! by construction, and an unedited document re-emits its exact input.
mod edit;
mod image;
mod part;

#[cfg(test)]
mod tests;

pub use image::{ImageEntry, ValueKind, is_animated_png};

use crate::{
    Compression, Limits, Movie, container,
    error::{Error, Result},
    movie::decode,
    wire::{self, Field, LENGTH_DELIMITED},
};
use part::Part;
use std::sync::Arc;

pub(crate) const VERSION: u32 = 1;
pub(crate) const PARAMS: u32 = 2;
pub(crate) const IMAGES: u32 = 3;
pub(crate) const SPRITES: u32 = 4;
pub(crate) const AUDIOS: u32 = 5;

/// An SVGA 2.x movie that can be edited and re-encoded without loss.
///
/// Edits return a new document and leave the receiver untouched; unedited
/// fields share their bytes, so this is cheap.
#[derive(Debug, Clone)]
pub struct Document {
    parts: Vec<Part>,
}

impl Document {
    /// Read an SVGA 2.x file: one zlib stream with nothing after it.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Self::from_bytes_with(bytes, &Limits::default())
    }

    pub fn from_bytes_with(bytes: &[u8], limits: &Limits) -> Result<Self> {
        Self::from_proto_with(container::inflate(bytes, limits)?, limits)
    }

    /// Read an uncompressed protobuf `MovieEntity`.
    pub fn from_proto(proto: impl Into<Arc<[u8]>>) -> Result<Self> {
        Self::from_proto_with(proto, &Limits::default())
    }

    pub fn from_proto_with(proto: impl Into<Arc<[u8]>>, limits: &Limits) -> Result<Self> {
        let buffer: Arc<[u8]> = proto.into();
        if buffer.len() > limits.max_inflated_bytes {
            return Err(Error::limit("svga_inflated_size_exceeds_limit"));
        }
        let mut parts = Vec::new();
        for field in wire::fields(&buffer) {
            if parts.len() >= limits.max_fields {
                return Err(Error::limit("svga_too_many_fields"));
            }
            parts.push(Part::from_source(&buffer, &field?)?);
        }
        Ok(Self { parts })
    }

    /// Build a new document from a typed movie, embedding `images` in order.
    /// `movie.images` is ignored: the map is exactly what is passed here.
    pub fn from_movie(movie: &Movie, images: &[(&str, &[u8])]) -> Result<Self> {
        Self::from_proto(crate::movie::encode::movie(movie, images))
    }

    /// Top-level fields in stored order, unknown ones included.
    pub fn fields(&self) -> impl Iterator<Item = Field<'_>> {
        self.parts.iter().map(Part::field)
    }

    /// Whether any top-level field is outside the SVGA 2.x schema.
    pub fn has_unknown_fields(&self) -> bool {
        self.parts
            .iter()
            .any(|part| !(VERSION..=AUDIOS).contains(&part.number))
    }

    /// The stored version string, without decoding anything else.
    pub fn version(&self) -> Option<&str> {
        let part = self.parts.iter().rfind(|part| part.number == VERSION)?;
        std::str::from_utf8(part.field().payload).ok()
    }

    /// Entries of the `images` map in stored order, duplicates included.
    pub fn images(&self) -> impl DoubleEndedIterator<Item = ImageEntry<'_>> {
        self.parts.iter().filter_map(Part::image)
    }

    /// The value stored under `key`. Like any protobuf map, the last entry
    /// wins when a key is repeated.
    pub fn image(&self, key: &str) -> Option<&[u8]> {
        let mut entries = self
            .images()
            .filter(|image| image.key_bytes() == key.as_bytes());
        entries.next_back().map(|image| image.value())
    }

    /// Decode the typed view.
    pub fn movie(&self) -> Result<Movie> {
        self.movie_with(&Limits::default())
    }

    pub fn movie_with(&self, limits: &Limits) -> Result<Movie> {
        decode::movie(self, limits)
    }

    /// Size of [`Document::to_proto`] without building it.
    pub fn proto_len(&self) -> usize {
        self.parts.iter().map(|part| part.raw().len()).sum()
    }

    /// The uncompressed protobuf. Byte-identical to the input when unedited.
    pub fn to_proto(&self) -> Vec<u8> {
        let mut proto = Vec::with_capacity(self.proto_len());
        for part in &self.parts {
            proto.extend_from_slice(part.raw());
        }
        proto
    }

    /// Encode as an SVGA 2.x file.
    pub fn to_bytes(&self, compression: Compression) -> Result<Vec<u8>> {
        container::deflate(&self.to_proto(), compression)
    }
}

fn expect_length_delimited(field: &Field<'_>) -> Result<()> {
    if field.wire_type == LENGTH_DELIMITED {
        Ok(())
    } else {
        Err(Error::malformed("svga_unexpected_wire_type"))
    }
}
