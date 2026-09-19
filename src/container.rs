//! SVGA 2.x container: one zlib stream holding a protobuf `MovieEntity`.
use crate::{
    Limits,
    error::{Error, Result},
};
use fdeflate::Decompressor;

const ZIP_MAGIC: &[u8] = b"PK";
/// The payload is mostly PNG data, which barely compresses: twice the input
/// almost always holds the output without a second allocation.
const OUTPUT_GUESS_FACTOR: usize = 2;
const MIN_OUTPUT_BYTES: usize = 4096;
/// More than the decoder can prefetch (its bit buffer holds 8 bytes).
const TAIL_BYTES: usize = 16;
const CORRUPT_STREAM: Error = Error::malformed("svga_corrupt_zlib_stream");
const TOO_LARGE: Error = Error::limit("svga_inflated_size_exceeds_limit");
const LEVEL_FAST: u8 = 1;
const LEVEL_DEFAULT: u8 = 6;
const LEVEL_BEST: u8 = 9;

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
    inflate_stream(bytes, limits.max_inflated_bytes)
}

/// Output being inflated into: one buffer, grown on demand up to `cap`.
struct Sink {
    bytes: Vec<u8>,
    produced: usize,
    cap: usize,
}

impl Sink {
    /// Feed all of `input`, then keep going until the decoder has nothing
    /// more to write, so that `is_done` reflects exactly the bytes given.
    fn pump(&mut self, decoder: &mut Decompressor, input: &[u8]) -> Result<()> {
        let mut rest = input;
        loop {
            let (read, written) = decoder
                .read(rest, &mut self.bytes, self.produced, false)
                .map_err(|_| CORRUPT_STREAM)?;
            rest = rest.get(read..).ok_or(CORRUPT_STREAM)?;
            self.produced = self.produced.checked_add(written).ok_or(CORRUPT_STREAM)?;
            if decoder.is_done() {
                // Whatever is left of `input` lies after the stream.
                return Ok(());
            }
            if self.produced >= self.bytes.len() {
                if self.bytes.len() >= self.cap {
                    return Err(TOO_LARGE);
                }
                let grown = self.bytes.len().saturating_mul(2).min(self.cap);
                self.bytes.resize(grown, 0);
            } else if read == 0 && written == 0 {
                // Settled. Input it refuses with room to write is a bad stream.
                return if rest.is_empty() {
                    Ok(())
                } else {
                    Err(CORRUPT_STREAM)
                };
            }
        }
    }
}

/// The inflater is driven directly rather than through a `Read` adapter: the
/// whole input is in memory, so it writes straight into one output buffer.
/// The Adler-32 checksum is verified.
///
/// The decoder prefetches up to a bit buffer of input, so the byte count it
/// reports cannot tell a stream that ends exactly at the end of the file from
/// one followed by a few stray bytes. The last [`TAIL_BYTES`] are therefore
/// fed one at a time: a decoder cannot finish on a byte it has not been given,
/// so finishing before the last byte means trailing data.
fn inflate_stream(bytes: &[u8], limit: usize) -> Result<Vec<u8>> {
    // One byte past the limit is enough to tell "too large" from "fits".
    let cap = limit.saturating_add(1);
    let guess = bytes.len().saturating_mul(OUTPUT_GUESS_FACTOR);
    let mut sink = Sink {
        bytes: vec![0; guess.clamp(MIN_OUTPUT_BYTES.min(cap), cap)],
        produced: 0,
        cap,
    };
    let mut decoder = Decompressor::new();
    let (head, tail) = bytes.split_at(bytes.len().saturating_sub(TAIL_BYTES));
    sink.pump(&mut decoder, head)?;
    let mut given = head.len();
    for byte in tail {
        if decoder.is_done() {
            break;
        }
        sink.pump(&mut decoder, std::slice::from_ref(byte))?;
        given += 1;
    }
    if !decoder.is_done() {
        return Err(CORRUPT_STREAM);
    }
    if sink.produced > limit {
        return Err(TOO_LARGE);
    }
    if given != bytes.len() {
        return Err(Error::malformed("svga_trailing_bytes"));
    }
    let Sink {
        mut bytes,
        produced,
        ..
    } = sink;
    bytes.truncate(produced);
    Ok(bytes)
}

#[cfg(feature = "libdeflate")]
const DEFLATE_FAILED: Error = Error::new(crate::ErrorKind::Encode, "svga_deflate_failed");

/// One zlib stream holding `bytes`.
pub(crate) fn deflate(bytes: &[u8], compression: Compression) -> Result<Vec<u8>> {
    #[cfg(feature = "libdeflate")]
    if compression == Compression::Best {
        return deflate_max(bytes);
    }
    let level = match compression {
        Compression::Fast => LEVEL_FAST,
        Compression::Default => LEVEL_DEFAULT,
        Compression::Best => LEVEL_BEST,
    };
    Ok(miniz_oxide::deflate::compress_to_vec_zlib(bytes, level))
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
