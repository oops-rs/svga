//! One entry point for every SVGA flavour, including the zip-based ones whose
//! images live next to the movie instead of inside it.
#[cfg(feature = "zip")]
mod spec;
#[cfg(feature = "zip")]
mod spec_shape;
#[cfg(feature = "zip")]
mod unzip;

#[cfg(all(test, feature = "zip"))]
mod tests;

use crate::{
    Container, Document, Limits, Movie, ValueKind,
    error::{Error, Result},
};

const IMAGE_EXTENSION: &str = ".png";

/// How the movie is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Format {
    /// SVGA 2.x: a zlib stream of the protobuf movie.
    Zlib,
    /// A zip holding the protobuf movie as `movie.binary`.
    ZipBinary,
    /// SVGA 1.x: a zip holding `movie.spec` JSON and loose images.
    ZipSpec,
}

/// A file inside a zip container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArchiveFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// A decoded animation of any supported flavour.
///
/// Zip containers need the `zip` feature (on by default); without it they are
/// refused as unsupported.
#[derive(Debug, Clone)]
pub struct Animation {
    format: Format,
    movie: Movie,
    document: Option<Document>,
    files: Vec<ArchiveFile>,
    /// SVGA 1.x only: image key → name of the container file holding it.
    names: Vec<(String, String)>,
}

impl Animation {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Self::from_bytes_with(bytes, &Limits::default())
    }

    pub fn from_bytes_with(bytes: &[u8], limits: &Limits) -> Result<Self> {
        if Container::detect(bytes) == Some(Container::Zip) {
            return Self::from_zip(bytes, limits);
        }
        let document = Document::from_bytes_with(bytes, limits)?;
        Ok(Self {
            format: Format::Zlib,
            movie: document.movie_with(limits)?,
            document: Some(document),
            files: Vec::new(),
            names: Vec::new(),
        })
    }

    #[cfg(not(feature = "zip"))]
    fn from_zip(_: &[u8], _: &Limits) -> Result<Self> {
        Err(Error::unsupported("svga_zip_support_disabled"))
    }

    #[cfg(feature = "zip")]
    fn from_zip(bytes: &[u8], limits: &Limits) -> Result<Self> {
        let files = unzip::extract(bytes, limits)?;
        let find = |name: &str| files.iter().find(|file| file.name == name);
        if let Some(binary) = find(unzip::MOVIE_BINARY) {
            let document = unzip::binary_document(&binary.bytes, limits)?;
            return Ok(Self {
                format: Format::ZipBinary,
                movie: document.movie_with(limits)?,
                document: Some(document),
                files,
                names: Vec::new(),
            });
        }
        let spec = find(unzip::MOVIE_SPEC).ok_or(Error::malformed("svga_zip_without_movie"))?;
        let (movie, names) = spec::movie(&spec.bytes, &files, limits)?;
        Ok(Self {
            format: Format::ZipSpec,
            movie,
            document: None,
            files,
            names,
        })
    }

    pub fn format(&self) -> Format {
        self.format
    }

    pub fn movie(&self) -> &Movie {
        &self.movie
    }

    /// The lossless document. `None` for SVGA 1.x, which has no protobuf.
    pub fn document(&self) -> Option<&Document> {
        self.document.as_ref()
    }

    /// The bytes of an image or audio clip, wherever this flavour keeps them:
    /// embedded in the movie, or in a container file named by the entry.
    pub fn image(&self, key: &str) -> Option<&[u8]> {
        match self.document.as_ref().map(|document| document.image(key)) {
            Some(Some(value)) if ValueKind::sniff(value) == ValueKind::FileName => {
                let name = std::str::from_utf8(value).ok()?;
                self.file(name).or(Some(value))
            }
            Some(stored) => stored,
            None => {
                let named = self.names.iter().find(|(known, _)| known == key);
                named.and_then(|(_, name)| self.file(name))
            }
        }
    }

    /// A container file called `name`, with or without the `.png` extension.
    fn file(&self, name: &str) -> Option<&[u8]> {
        find_file(&self.files, name)
    }

    /// A self-contained SVGA 2.x document: images kept in container files are
    /// embedded, and SVGA 1.x is converted. Images whose file is missing stay
    /// as they are (2.x) or are left out (1.x). Converting 1.x re-encodes the
    /// movie from the typed view; 2.x input keeps every other byte.
    pub fn to_document(&self) -> Result<Document> {
        let Some(document) = &self.document else {
            let images: Vec<(&str, &[u8])> = self
                .movie
                .images
                .iter()
                .filter_map(|image| Some((image.key.as_str(), self.image(&image.key)?)))
                .collect();
            return Document::from_movie(&self.movie, &images);
        };
        let external: Vec<(&str, &[u8])> = document
            .images()
            .filter(|image| image.kind() == ValueKind::FileName)
            .filter_map(|image| {
                let name = std::str::from_utf8(image.value()).ok()?;
                Some((image.key()?, self.file(name)?))
            })
            .collect();
        external
            .iter()
            .try_fold(document.clone(), |document, (key, bytes)| {
                document.replace_image(key, bytes)
            })
    }
}

pub(crate) fn find_file<'a>(files: &'a [ArchiveFile], name: &str) -> Option<&'a [u8]> {
    let with_extension = format!("{name}{IMAGE_EXTENSION}");
    files
        .iter()
        .find(|file| file.name == name || file.name == with_extension)
        .map(|file| file.bytes.as_slice())
}
