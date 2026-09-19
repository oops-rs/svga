//! Bounded extraction of a zip container into memory.
use super::ArchiveFile;
use crate::{
    Document, Limits, container,
    error::{Error, Result},
};
use std::io::{Cursor, Read};
use zip::{ZipArchive, result::ZipError};

pub(super) const MOVIE_BINARY: &str = "movie.binary";
pub(super) const MOVIE_SPEC: &str = "movie.spec";

fn zip_error(error: ZipError) -> Error {
    match error {
        ZipError::UnsupportedArchive(_) => Error::unsupported("svga_unsupported_zip"),
        _ => Error::malformed("svga_corrupt_zip"),
    }
}

/// Every regular file of the archive. The total extracted size counts against
/// `max_inflated_bytes`, whatever the entry headers claim.
pub(super) fn extract(bytes: &[u8], limits: &Limits) -> Result<Vec<ArchiveFile>> {
    if bytes.len() > limits.max_input_bytes {
        return Err(Error::limit("svga_input_exceeds_limit"));
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(zip_error)?;
    if archive.len() > limits.max_archive_entries {
        return Err(Error::limit("svga_too_many_archive_entries"));
    }
    let mut files = Vec::new();
    let mut remaining = limits.max_inflated_bytes;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(zip_error)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_owned();
        let cap = u64::try_from(remaining)
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        let mut contents = Vec::new();
        entry
            .take(cap)
            .read_to_end(&mut contents)
            .map_err(|_| Error::malformed("svga_corrupt_zip"))?;
        remaining = remaining
            .checked_sub(contents.len())
            .ok_or(Error::limit("svga_inflated_size_exceeds_limit"))?;
        files.push(ArchiveFile {
            name,
            bytes: contents,
        });
    }
    Ok(files)
}

/// `movie.binary` is the bare protobuf; a zlib-wrapped one is accepted too
/// (a real movie starts with field 1, never with a zlib header). Inflating
/// draws on what the `extracted` container files left of the budget.
pub(super) fn binary_document(bytes: &[u8], limits: &Limits, extracted: usize) -> Result<Document> {
    if !container::is_zlib_header(bytes) {
        return Document::from_proto_with(bytes, limits);
    }
    let remaining = limits.max_inflated_bytes.saturating_sub(extracted);
    Document::from_bytes_with(bytes, &limits.with_max_inflated_bytes(remaining))
}
