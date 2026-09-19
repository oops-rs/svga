//! SVGA 2.x container: one zlib stream holding a protobuf `MovieEntity`.
use crate::{
    Limits,
    error::{Error, Result},
};
use std::io::{Read, Write};

const ZIP_MAGIC: &[u8] = b"PK";

/// How a file wraps its movie, judged from the leading bytes only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Container {
    /// SVGA 2.x: a zlib stream of the protobuf movie.
    Zlib,
    /// A zip archive: SVGA 1.x (`movie.spec`) or zipped 2.x (`movie.binary`).
    Zip,
}

impl Container {
    pub fn detect(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(ZIP_MAGIC) {
            Some(Self::Zip)
        } else if is_zlib_header(bytes) {
            Some(Self::Zlib)
        } else {
            None
        }
    }
}

/// Effort spent compressing the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum Compression {
    Fast,
    #[default]
    Default,
    /// The smallest output this build can produce: libdeflate at its highest
    /// level with the `libdeflate` feature, otherwise the best pure-Rust level.
    Best,
}

pub(crate) fn is_zlib_header(bytes: &[u8]) -> bool {
    match bytes {
        [cmf, flags, ..] => {
            cmf & 0x0f == 8 && (u16::from(*cmf) << 8 | u16::from(*flags)).is_multiple_of(31)
        }
        _ => false,
    }
}

/// Inflate the whole file, refusing anything that is not exactly one zlib
/// stream or that expands beyond the limits.
pub(crate) fn inflate(bytes: &[u8], limits: &Limits) -> Result<Vec<u8>> {
    if bytes.len() > limits.max_input_bytes {
        return Err(Error::limit("svga_input_exceeds_limit"));
    }
    match Container::detect(bytes) {
        Some(Container::Zlib) => {}
        Some(Container::Zip) => return Err(Error::unsupported("svga_zip_container")),
        None => return Err(Error::malformed("svga_not_zlib")),
    }
    // `&[u8]` is already buffered, so the decoder never reads past the stream
    // end and `total_in` is the exact stream length.
    let mut decoder = flate2::bufread::ZlibDecoder::new(bytes);
    let mut inflated = Vec::new();
    let limit = limits.max_inflated_bytes;
    let cap = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
    decoder
        .by_ref()
        .take(cap)
        .read_to_end(&mut inflated)
        .map_err(|_| Error::malformed("svga_corrupt_zlib_stream"))?;
    if inflated.len() > limit {
        return Err(Error::limit("svga_inflated_size_exceeds_limit"));
    }
    if decoder.total_in() != bytes.len() as u64 {
        return Err(Error::malformed("svga_trailing_bytes"));
    }
    Ok(inflated)
}

const DEFLATE_FAILED: Error = Error::new(crate::ErrorKind::Encode, "svga_deflate_failed");

/// One zlib stream holding `bytes`.
pub(crate) fn deflate(bytes: &[u8], compression: Compression) -> Result<Vec<u8>> {
    #[cfg(feature = "libdeflate")]
    if compression == Compression::Best {
        return deflate_max(bytes);
    }
    let level = match compression {
        Compression::Fast => flate2::Compression::fast(),
        Compression::Default => flate2::Compression::default(),
        Compression::Best => flate2::Compression::best(),
    };
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), level);
    encoder.write_all(bytes).map_err(|_| DEFLATE_FAILED)?;
    encoder.finish().map_err(|_| DEFLATE_FAILED)
}

#[cfg(feature = "libdeflate")]
fn deflate_max(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut compressor = libdeflater::Compressor::new(libdeflater::CompressionLvl::best());
    let mut compressed = vec![0; compressor.zlib_compress_bound(bytes.len())];
    let length = compressor
        .zlib_compress(bytes, &mut compressed)
        .map_err(|_| DEFLATE_FAILED)?;
    compressed.truncate(length);
    Ok(compressed)
}
