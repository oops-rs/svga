//! A codec for the [SVGA] animation format: decode, inspect and losslessly
//! edit. It renders nothing and makes no optimization decisions.
//!
//! There are two layers:
//!
//! - [`Document`] is the lossless one. It keeps every top-level protobuf field
//!   as its original bytes, so unknown fields, field order and float bit
//!   patterns survive, and an unedited document re-encodes to the exact same
//!   payload. All edits happen here.
//! - [`Movie`] is a typed, read-only view decoded on demand from a document.
//!
//! ```
//! # fn main() -> Result<(), svga::Error> {
//! # let movie = svga::Movie { version: "2.0.0".into(), ..Default::default() };
//! # let file = svga::Document::from_movie(&movie, &[("hat", b"\x89PNG\r\n\x1a\n".as_slice())])?
//! #     .to_bytes(svga::Compression::Fast)?;
//! let document = svga::Document::from_bytes(&file)?;
//! for image in document.images() {
//!     println!("{:?}: {} bytes", image.key(), image.value().len());
//! }
//! let smaller = document.replace_image("hat", b"hat.png")?;
//! let bytes = smaller.to_bytes(svga::Compression::Best)?;
//! # assert!(!bytes.is_empty());
//! # Ok(())
//! # }
//! ```
//!
//! [SVGA]: https://github.com/svga/SVGA-Format
#![forbid(unsafe_code)]

mod archive;
mod container;
mod document;
mod error;
#[cfg(test)]
mod fixtures;
mod limits;
pub mod movie;
pub mod wire;

pub use archive::{Animation, Format};
pub use container::{Compression, Container};
pub use document::{Document, ImageEntry, ValueKind, is_animated_png, png_dimensions};
pub use error::{Error, ErrorKind, Result};
pub use limits::Limits;
pub use movie::Movie;
