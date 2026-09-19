//! A leaner walk for typed decoding: each field's value is decoded as it is
//! met, and nothing is kept of the field's raw bytes. The lossless layer uses
//! [`fields`](super::fields) instead, which keeps them.
use super::{
    FIXED32, FIXED64, LENGTH_DELIMITED, MAX_FIELD_NUMBER, TRUNCATED_FIELD, VARINT, varint,
};
use crate::error::{Error, Result};

const UNEXPECTED_WIRE_TYPE: Error = Error::malformed("svga_unexpected_wire_type");

#[derive(Debug, Clone, Copy)]
enum Value<'a> {
    Varint(u64),
    Fixed32([u8; 4]),
    Fixed64,
    Bytes(&'a [u8]),
}

/// One decoded field.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Item<'a> {
    pub number: u32,
    value: Value<'a>,
}

impl<'a> Item<'a> {
    #[inline]
    pub fn bytes(&self) -> Result<&'a [u8]> {
        match self.value {
            Value::Bytes(bytes) => Ok(bytes),
            _ => Err(UNEXPECTED_WIRE_TYPE),
        }
    }

    pub fn str(&self) -> Result<&'a str> {
        std::str::from_utf8(self.bytes()?).map_err(|_| Error::malformed("svga_invalid_utf8"))
    }

    #[inline]
    pub fn f32(&self) -> Result<f32> {
        match self.value {
            Value::Fixed32(bytes) => Ok(f32::from_le_bytes(bytes)),
            _ => Err(UNEXPECTED_WIRE_TYPE),
        }
    }

    /// An `int32`: negative values are stored sign-extended to 64 bits.
    #[inline]
    pub fn i32(&self) -> Result<i32> {
        match self.value {
            Value::Varint(value) => Ok(value as i32),
            _ => Err(UNEXPECTED_WIRE_TYPE),
        }
    }
}

/// Iterator over the decoded fields of one message. Stops after an error.
pub(crate) struct Values<'a> {
    bytes: &'a [u8],
    offset: usize,
}

pub(crate) fn values(bytes: &[u8]) -> Values<'_> {
    Values { bytes, offset: 0 }
}

impl<'a> Values<'a> {
    #[inline]
    fn read(&self) -> Result<(Item<'a>, usize)> {
        let bytes = self.bytes;
        let (tag, at) = varint(bytes, self.offset)?;
        let number = tag >> 3;
        if number == 0 || number > MAX_FIELD_NUMBER {
            return Err(Error::malformed("svga_invalid_field_number"));
        }
        let slice = |start: usize, length: usize| {
            let end = start.checked_add(length).ok_or(TRUNCATED_FIELD)?;
            Ok((bytes.get(start..end).ok_or(TRUNCATED_FIELD)?, end))
        };
        let (value, end) = match (tag & 7) as u8 {
            VARINT => {
                let (value, end) = varint(bytes, at)?;
                (Value::Varint(value), end)
            }
            FIXED32 => {
                let (raw, end) = slice(at, 4)?;
                let raw = raw.first_chunk::<4>().ok_or(TRUNCATED_FIELD)?;
                (Value::Fixed32(*raw), end)
            }
            FIXED64 => (Value::Fixed64, slice(at, 8)?.1),
            LENGTH_DELIMITED => {
                let (length, start) = varint(bytes, at)?;
                let length = usize::try_from(length).map_err(|_| TRUNCATED_FIELD)?;
                let (body, end) = slice(start, length)?;
                (Value::Bytes(body), end)
            }
            _ => return Err(Error::malformed("svga_unsupported_wire_type")),
        };
        let number = number as u32;
        Ok((Item { number, value }, end))
    }
}

impl<'a> Iterator for Values<'a> {
    type Item = Result<Item<'a>>;

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
        Some(item.map(|(item, _)| item))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{fields, float, int32, length_delimited};

    /// Both walkers must accept and reject exactly the same messages.
    #[test]
    fn values_agree_with_fields_on_every_prefix_and_mutation() {
        let message = [
            float(1, 1.5),
            int32(2, -3),
            length_delimited(3, b"abc"),
            vec![0x21, 1, 2, 3, 4, 5, 6, 7, 8],
            length_delimited(1 << 20, &[]),
        ]
        .concat();
        let mut cases: Vec<Vec<u8>> = (0..=message.len()).map(|n| message[..n].to_vec()).collect();
        for index in 0..message.len() {
            for bit in 0..8 {
                let mut mutated = message.clone();
                mutated[index] ^= 1 << bit;
                cases.push(mutated);
            }
        }
        for case in cases {
            let lean: Vec<_> = values(&case)
                .map(|item| item.map(|item| item.number))
                .collect();
            let full: Vec<_> = fields(&case).map(|field| field.map(|f| f.number)).collect();
            assert_eq!(lean, full, "{case:?}");
        }
    }

    #[test]
    fn values_are_typed() {
        let message = [float(1, 1.5), int32(2, -3), length_delimited(3, &[0xff])].concat();
        let items: Vec<_> = values(&message).map(Result::unwrap).collect();
        assert_eq!(items[0].f32().unwrap(), 1.5);
        assert_eq!(items[1].i32().unwrap(), -3);
        assert_eq!(items[2].bytes().unwrap(), [0xff]);
        assert_eq!(items[2].str().unwrap_err().code(), "svga_invalid_utf8");
        for wrong in [
            items[0].i32().err(),
            items[1].f32().err(),
            items[0].bytes().err(),
        ] {
            assert_eq!(wrong.unwrap().code(), "svga_unexpected_wire_type");
        }
    }
}
