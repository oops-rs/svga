//! One top-level field. Its bytes are a window into a shared buffer: the
//! inflated source for untouched fields, a small owned buffer for edited ones.
use super::{AUDIOS, IMAGES, ImageEntry, VERSION, expect_length_delimited};
use crate::{
    error::Result,
    wire::{self, Field, LENGTH_DELIMITED},
};
use std::{ops::Range, sync::Arc};

const ENTRY_KEY: u32 = 1;
const ENTRY_VALUE: u32 = 2;

#[derive(Debug, Clone)]
pub(super) struct Part {
    pub number: u32,
    wire_type: u8,
    buffer: Arc<[u8]>,
    raw: Range<usize>,
    payload: Range<usize>,
    entry: Option<Entry>,
}

/// Where a map entry keeps its key and value, as ranges into the buffer.
#[derive(Debug, Clone)]
struct Entry {
    key: Range<usize>,
    value: Range<usize>,
    numbers: Vec<u32>,
}

fn shifted(range: Range<usize>, base: usize) -> Range<usize> {
    range.start + base..range.end + base
}

impl Part {
    /// A part viewing `field`, which must come from walking `buffer`.
    pub fn from_source(buffer: &Arc<[u8]>, field: &Field<'_>) -> Result<Self> {
        if (VERSION..=AUDIOS).contains(&field.number) {
            expect_length_delimited(field)?;
        }
        let entry = (field.number == IMAGES)
            .then(|| Entry::parse(field.payload, field.payload_range().start))
            .transpose()?;
        Ok(Self {
            number: field.number,
            wire_type: field.wire_type,
            buffer: Arc::clone(buffer),
            raw: field.offset..field.offset + field.raw.len(),
            payload: field.payload_range(),
            entry,
        })
    }

    /// A new `images` entry holding `entry_payload`.
    pub fn image_entry(entry_payload: &[u8]) -> Result<Self> {
        let buffer: Arc<[u8]> = wire::length_delimited(IMAGES, entry_payload).into();
        let field = wire::fields(&buffer)
            .next()
            .transpose()?
            .ok_or(crate::Error::malformed("svga_truncated_field"))?;
        Self::from_source(&buffer, &field)
    }

    pub fn raw(&self) -> &[u8] {
        self.buffer.get(self.raw.clone()).unwrap_or_default()
    }

    fn slice(&self, range: &Range<usize>) -> &[u8] {
        self.buffer.get(range.clone()).unwrap_or_default()
    }

    pub fn field(&self) -> Field<'_> {
        Field {
            number: self.number,
            wire_type: self.wire_type,
            offset: self.raw.start,
            raw: self.raw(),
            payload: self.slice(&self.payload),
        }
    }

    pub fn image(&self) -> Option<ImageEntry<'_>> {
        let entry = self.entry.as_ref()?;
        Some(ImageEntry {
            key: self.slice(&entry.key),
            value: self.slice(&entry.value),
            field: self.field(),
            numbers: &entry.numbers,
        })
    }
}

impl Entry {
    /// Protobuf map semantics: the last key and the last value win, and a
    /// missing one is empty. `base` is where `payload` starts in the buffer.
    fn parse(payload: &[u8], base: usize) -> Result<Self> {
        let end = base + payload.len();
        let (mut key, mut value) = (end..end, end..end);
        let mut numbers = Vec::with_capacity(2);
        for field in wire::fields(payload) {
            let field = field?;
            numbers.push(field.number);
            if matches!(field.number, ENTRY_KEY | ENTRY_VALUE) {
                expect_length_delimited(&field)?;
                let range = shifted(field.payload_range(), base);
                if field.number == ENTRY_KEY {
                    key = range;
                } else {
                    value = range;
                }
            }
        }
        Ok(Self {
            key,
            value,
            numbers,
        })
    }
}

/// The payload of a map entry for `key` and `value`.
pub(super) fn entry_payload(key: &[u8], value: &[u8]) -> Vec<u8> {
    [
        wire::length_delimited(ENTRY_KEY, key),
        wire::length_delimited(ENTRY_VALUE, value),
    ]
    .concat()
}

/// `entry` with its value replaced. Every other entry field, known or not,
/// keeps its bytes and position; only the winning (last) value is rewritten.
pub(super) fn entry_with_value(entry: &[u8], value: &[u8]) -> Result<Vec<u8>> {
    let fields = wire::walk(entry)?;
    let last = fields
        .iter()
        .rposition(|field| field.number == ENTRY_VALUE && field.wire_type == LENGTH_DELIMITED);
    let replacement = wire::length_delimited(ENTRY_VALUE, value);
    let mut rebuilt: Vec<u8> = fields
        .iter()
        .enumerate()
        .flat_map(|(index, field)| {
            if Some(index) == last {
                replacement.as_slice()
            } else {
                field.raw
            }
        })
        .copied()
        .collect();
    if last.is_none() {
        rebuilt.extend_from_slice(&replacement);
    }
    Ok(rebuilt)
}
