//! Minimal protobuf wire walker and writer.
//!
//! Walking never interprets a field: each one keeps its original bytes, which
//! is what lets untouched data be copied verbatim.
mod values;

pub(crate) use values::{Item, values};

use crate::error::{Error, Result};

pub const VARINT: u8 = 0;
pub const FIXED64: u8 = 1;
pub const LENGTH_DELIMITED: u8 = 2;
pub const FIXED32: u8 = 5;

const MAX_FIELD_NUMBER: u64 = (1 << 29) - 1;
const MAX_VARINT_BYTES: usize = 10;
const TRUNCATED_VARINT: Error = Error::malformed("svga_truncated_varint");
const TRUNCATED_FIELD: Error = Error::malformed("svga_truncated_field");
const VARINT_OVERFLOW: Error = Error::malformed("svga_varint_overflow");

/// One field of a message, borrowed from the message bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field<'a> {
    pub number: u32,
    pub wire_type: u8,
    /// Position of `raw` within the walked message.
    pub(crate) offset: usize,
    /// Tag, length prefix and payload exactly as stored.
    pub raw: &'a [u8],
    /// The value: varint bytes, fixed bytes, or the length-delimited body.
    pub payload: &'a [u8],
}

impl<'a> Field<'a> {
    /// Where `payload` sits within the walked message.
    pub(crate) fn payload_range(&self) -> std::ops::Range<usize> {
        let end = self.offset + self.raw.len();
        end - self.payload.len()..end
    }

    /// The body of a length-delimited field.
    pub fn bytes(&self) -> Result<&'a [u8]> {
        self.expect(LENGTH_DELIMITED).map(|()| self.payload)
    }

    pub fn str(&self) -> Result<&'a str> {
        std::str::from_utf8(self.bytes()?).map_err(|_| Error::malformed("svga_invalid_utf8"))
    }

    pub fn f32(&self) -> Result<f32> {
        self.expect(FIXED32)?;
        let bytes: [u8; 4] = self.payload.try_into().map_err(|_| TRUNCATED_FIELD)?;
        Ok(f32::from_le_bytes(bytes))
    }

    pub fn u64(&self) -> Result<u64> {
        self.expect(VARINT)?;
        Ok(varint(self.payload, 0)?.0)
    }

    /// An `int32`: negative values are stored sign-extended to 64 bits.
    pub fn i32(&self) -> Result<i32> {
        Ok(self.u64()? as i32)
    }

    fn expect(&self, wire_type: u8) -> Result<()> {
        if self.wire_type == wire_type {
            Ok(())
        } else {
            Err(Error::malformed("svga_unexpected_wire_type"))
        }
    }
}

#[inline]
fn varint(bytes: &[u8], at: usize) -> Result<(u64, usize)> {
    // Tags and most lengths fit one byte; skip the loop for those.
    if let Some(&byte) = bytes.get(at)
        && byte < 0x80
    {
        return Ok((u64::from(byte), at + 1));
    }
    varint_slow(bytes, at)
}

fn varint_slow(bytes: &[u8], at: usize) -> Result<(u64, usize)> {
    let mut value = 0u64;
    for index in 0..MAX_VARINT_BYTES {
        let position = at.checked_add(index).ok_or(TRUNCATED_VARINT)?;
        let byte = *bytes.get(position).ok_or(TRUNCATED_VARINT)?;
        let bits = u64::from(byte & 0x7f);
        // The tenth byte only has room for the top bit of a u64.
        if index == MAX_VARINT_BYTES - 1 && bits > 1 {
            return Err(VARINT_OVERFLOW);
        }
        value |= bits << (7 * index);
        if byte & 0x80 == 0 {
            return Ok((value, position + 1));
        }
    }
    Err(VARINT_OVERFLOW)
}

/// Iterator over the fields of one message. Stops after the first error.
#[derive(Debug, Clone)]
pub struct Fields<'a> {
    bytes: &'a [u8],
    offset: usize,
}

/// Iterate the fields of `bytes` without interpreting nested messages.
pub fn fields(bytes: &[u8]) -> Fields<'_> {
    Fields { bytes, offset: 0 }
}

/// All fields of one message, or the first error.
pub fn walk(bytes: &[u8]) -> Result<Vec<Field<'_>>> {
    fields(bytes).collect()
}

impl<'a> Fields<'a> {
    #[inline]
    fn read(&self) -> Result<(Field<'a>, usize)> {
        let (bytes, offset) = (self.bytes, self.offset);
        let (tag, after_tag) = varint(bytes, offset)?;
        let number = tag >> 3;
        if number == 0 || number > MAX_FIELD_NUMBER {
            return Err(Error::malformed("svga_invalid_field_number"));
        }
        let wire_type = (tag & 7) as u8;
        let (start, end) = match wire_type {
            VARINT => (after_tag, varint(bytes, after_tag)?.1),
            FIXED64 => (after_tag, after_tag.checked_add(8).ok_or(TRUNCATED_FIELD)?),
            FIXED32 => (after_tag, after_tag.checked_add(4).ok_or(TRUNCATED_FIELD)?),
            LENGTH_DELIMITED => {
                let (length, start) = varint(bytes, after_tag)?;
                let length = usize::try_from(length).map_err(|_| TRUNCATED_FIELD)?;
                (start, start.checked_add(length).ok_or(TRUNCATED_FIELD)?)
            }
            _ => return Err(Error::malformed("svga_unsupported_wire_type")),
        };
        let field = Field {
            number: number as u32,
            wire_type,
            offset,
            raw: bytes.get(offset..end).ok_or(TRUNCATED_FIELD)?,
            payload: bytes.get(start..end).ok_or(TRUNCATED_FIELD)?,
        };
        Ok((field, end))
    }
}

impl<'a> Iterator for Fields<'a> {
    type Item = Result<Field<'a>>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.bytes.len() {
            return None;
        }
        let item = self.read();
        self.offset = match &item {
            Ok((_, end)) => *end,
            Err(_) => self.bytes.len(),
        };
        Some(item.map(|(field, _)| field))
    }
}

fn encode_varint(value: u64) -> Vec<u8> {
    let groups = (u64::BITS - value.leading_zeros()).div_ceil(7).max(1);
    (0..groups)
        .map(|group| {
            let bits = ((value >> (7 * group)) & 0x7f) as u8;
            if group + 1 < groups {
                bits | 0x80
            } else {
                bits
            }
        })
        .collect()
}

fn tag(number: u32, wire_type: u8) -> Vec<u8> {
    encode_varint((u64::from(number) << 3) | u64::from(wire_type))
}

/// A length-delimited field with a canonical tag and length prefix.
pub fn length_delimited(number: u32, payload: &[u8]) -> Vec<u8> {
    [
        tag(number, LENGTH_DELIMITED),
        encode_varint(payload.len() as u64),
        payload.to_vec(),
    ]
    .concat()
}

/// A `float` field.
pub fn float(number: u32, value: f32) -> Vec<u8> {
    [tag(number, FIXED32), value.to_le_bytes().to_vec()].concat()
}

/// An `int32` or enum field; negative values are sign-extended to 64 bits.
pub fn int32(number: u32, value: i32) -> Vec<u8> {
    [tag(number, VARINT), encode_varint(i64::from(value) as u64)].concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_round_trip_at_every_width() {
        for value in [0, 1, 127, 128, 300, u64::from(u32::MAX), u64::MAX] {
            let bytes = encode_varint(value);
            assert_eq!(varint(&bytes, 0).unwrap(), (value, bytes.len()));
        }
        assert_eq!(encode_varint(u64::MAX).len(), MAX_VARINT_BYTES);
    }

    #[test]
    fn negative_int32_is_ten_bytes_and_decodes_back() {
        let bytes = int32(3, -7);
        let field = walk(&bytes).unwrap()[0];
        assert_eq!(field.raw.len(), 11);
        assert_eq!(field.i32().unwrap(), -7);
    }

    #[test]
    fn typed_getters_reject_the_wrong_wire_type() {
        let bytes = float(1, 1.5);
        let field = walk(&bytes).unwrap()[0];
        assert_eq!(field.f32().unwrap(), 1.5);
        let code = field.u64().unwrap_err().code();
        assert_eq!(code, "svga_unexpected_wire_type");
        assert_eq!(field.bytes().unwrap_err().code(), code);
        let text = length_delimited(1, &[0xff]);
        let field = walk(&text).unwrap()[0];
        assert_eq!(field.str().unwrap_err().code(), "svga_invalid_utf8");
    }

    #[test]
    fn the_iterator_ends_after_the_first_error() {
        let bytes = [float(1, 1.0), vec![0x0a, 0x7f, 1]].concat();
        let mut iterator = fields(&bytes);
        assert!(iterator.next().unwrap().is_ok());
        assert_eq!(
            iterator.next().unwrap().unwrap_err().code(),
            "svga_truncated_field"
        );
        assert!(iterator.next().is_none());
    }
}
