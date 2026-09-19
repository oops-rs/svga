//! Read-only view of one `images` map entry, plus value sniffing.
use crate::wire::Field;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const MAX_FILE_NAME_BYTES: usize = 255;
const CANONICAL_ENTRY_FIELDS: usize = 2;

/// What an `images` value looks like. Sniffed from the bytes; nothing is decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ValueKind {
    /// Starts with the PNG signature. May still be an APNG, see
    /// [`is_animated_png`].
    Png,
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
    value.len() <= MAX_FILE_NAME_BYTES
        && std::str::from_utf8(value).is_ok_and(|name| !name.chars().any(char::is_control))
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
