//! Read-only view of one `images` map entry, plus value sniffing.
use crate::wire::Field;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const JPEG_SIGNATURE: &[u8] = &[0xff, 0xd8, 0xff];
const PNG_HEADER: &[u8] = b"IHDR";
const CHUNK_LENGTH_BYTES: usize = 4;
const MAX_FILE_NAME_BYTES: usize = 255;
const CANONICAL_ENTRY_FIELDS: usize = 2;

/// What an `images` value looks like. Sniffed from the bytes; nothing is decoded.
///
/// This is a heuristic for listing and reporting, checked in the order of the
/// variants: short text that happens to start with `ID3` reads as [`Mp3`], and
/// an empty value is [`Unknown`] rather than a file name. Callers with a
/// stricter notion of a file name should test the bytes themselves.
///
/// [`Mp3`]: ValueKind::Mp3
/// [`Unknown`]: ValueKind::Unknown
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ValueKind {
    /// Starts with the PNG signature. May still be an APNG, see
    /// [`is_animated_png`].
    Png,
    /// Starts with a JPEG start-of-image marker. Rare; some players accept it.
    Jpeg,
    /// MP3 audio, referenced by an audio entity.
    Mp3,
    /// Short printable UTF-8: the name of a file shipped next to the movie.
    FileName,
    Unknown,
}

impl ValueKind {
    pub fn sniff(value: &[u8]) -> Self {
        if value.starts_with(PNG_SIGNATURE) {
            Self::Png
        } else if value.starts_with(JPEG_SIGNATURE) {
            Self::Jpeg
        } else if is_mp3(value) {
            Self::Mp3
        } else if is_file_name(value) {
            Self::FileName
        } else {
            Self::Unknown
        }
    }
}

/// An ID3v2 tag or an MPEG audio frame sync.
fn is_mp3(value: &[u8]) -> bool {
    match value {
        [b'I', b'D', b'3', ..] => true,
        [0xff, second, ..] => second & 0xe0 == 0xe0,
        _ => false,
    }
}

fn is_file_name(value: &[u8]) -> bool {
    !value.is_empty()
        && value.len() <= MAX_FILE_NAME_BYTES
        && std::str::from_utf8(value).is_ok_and(|name| !name.chars().any(char::is_control))
}

/// Pixel width and height from a PNG's header, without decoding anything.
/// `None` when the bytes are not a PNG with a leading `IHDR` chunk.
pub fn png_dimensions(png: &[u8]) -> Option<(u32, u32)> {
    let header = png.strip_prefix(PNG_SIGNATURE)?.get(CHUNK_LENGTH_BYTES..)?;
    let (width, rest) = header.strip_prefix(PNG_HEADER)?.split_first_chunk::<4>()?;
    let (height, _) = rest.split_first_chunk::<4>()?;
    Some((u32::from_be_bytes(*width), u32::from_be_bytes(*height)))
}

/// Whether a PNG declares animation. `acTL` must precede `IDAT`, so the walk
/// stops at the first image data.
pub fn is_animated_png(png: &[u8]) -> bool {
    if !png.starts_with(PNG_SIGNATURE) {
        return false;
    }
    let mut offset = PNG_SIGNATURE.len();
    while let Some(header) = offset.checked_add(8).and_then(|end| png.get(offset..end)) {
        let Some((length, kind)) = header.split_first_chunk::<4>() else {
            return false;
        };
        match kind {
            b"acTL" => return true,
            b"IDAT" | b"IEND" => return false,
            _ => {}
        }
        // Length, type, data and CRC.
        let next = usize::try_from(u32::from_be_bytes(*length))
            .ok()
            .and_then(|length| length.checked_add(12))
            .and_then(|size| offset.checked_add(size));
        let Some(next) = next else { return false };
        offset = next;
    }
    false
}

/// One entry of the `images` map as stored.
#[derive(Debug, Clone, Copy)]
pub struct ImageEntry<'a> {
    pub(super) key: &'a [u8],
    pub(super) value: &'a [u8],
    pub(super) field: Field<'a>,
    pub(super) numbers: &'a [u32],
}

impl<'a> ImageEntry<'a> {
    /// The key, when it is valid UTF-8 (the schema says it should be).
    pub fn key(&self) -> Option<&'a str> {
        std::str::from_utf8(self.key).ok()
    }

    pub fn key_bytes(&self) -> &'a [u8] {
        self.key
    }

    /// Raw PNG or MP3 bytes, or a short file name.
    pub fn value(&self) -> &'a [u8] {
        self.value
    }

    pub fn kind(&self) -> ValueKind {
        ValueKind::sniff(self.value)
    }

    /// The whole top-level `images` field this entry is stored in.
    pub fn field(&self) -> Field<'a> {
        self.field
    }

    /// Field numbers inside the entry, in stored order. Normally `[1, 2]`.
    pub fn field_numbers(&self) -> &'a [u32] {
        self.numbers
    }

    /// Exactly one key and one value, in either order, and nothing else.
    pub fn is_canonical(&self) -> bool {
        self.numbers.len() == CANONICAL_ENTRY_FIELDS
            && self.numbers.contains(&1)
            && self.numbers.contains(&2)
    }
}
