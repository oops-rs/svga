//! Hand-encoded protobuf for tests. Real SVGA files never carry audio, unknown
//! fields or duplicate keys, so those paths only exist here.
use crate::wire::{self, float, int32};
use std::io::Write;

pub const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

pub fn field(number: u32, payload: &[u8]) -> Vec<u8> {
    wire::length_delimited(number, payload)
}

/// Bytes that look like a PNG. Nothing in this crate decodes pixels.
pub fn png(seed: u8) -> Vec<u8> {
    [PNG_SIGNATURE, &[seed; 64]].concat()
}

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let length = (data.len() as u32).to_be_bytes();
    [&length[..], kind, data, &[0; 4]].concat()
}

pub fn animated_png() -> Vec<u8> {
    [
        PNG_SIGNATURE.to_vec(),
        chunk(b"IHDR", &[0; 13]),
        chunk(b"acTL", &[0; 8]),
        chunk(b"IDAT", &[0; 4]),
    ]
    .concat()
}

pub fn still_png() -> Vec<u8> {
    [
        PNG_SIGNATURE.to_vec(),
        chunk(b"IHDR", &[0; 13]),
        chunk(b"IDAT", &[0; 4]),
        chunk(b"acTL", &[0; 8]),
    ]
    .concat()
}

pub fn image_entry(key: &str, value: &[u8]) -> Vec<u8> {
    field(3, &[field(1, key.as_bytes()), field(2, value)].concat())
}

pub fn layout(width: f32, height: f32) -> Vec<u8> {
    field(2, &[float(3, width), float(4, height)].concat())
}

/// A sprite whose frame carries field 15 and which itself carries field 99,
/// neither of which exists in the SVGA schema.
pub fn sprite(key: &str, alpha: f32) -> Vec<u8> {
    let frame = [float(1, alpha), layout(64.0, 64.0), int32(15, 42)].concat();
    let body = [
        field(1, key.as_bytes()),
        field(2, &frame),
        field(2, &frame),
        field(99, b"vendor extension"),
    ]
    .concat();
    field(4, &body)
}

pub fn header() -> Vec<u8> {
    let params = [float(1, 64.0), float(2, 48.0), int32(3, 20), int32(4, 2)].concat();
    [field(1, b"2.1.0"), field(2, &params)].concat()
}

pub fn movie_with(images: &[(&str, Vec<u8>)], tail: &[u8]) -> Vec<u8> {
    let entries: Vec<u8> = images
        .iter()
        .flat_map(|(key, value)| image_entry(key, value))
        .collect();
    [header(), entries, sprite("img_0", 1.0), tail.to_vec()].concat()
}

pub fn images() -> Vec<(&'static str, Vec<u8>)> {
    vec![("img_0", png(1)), ("img_1", png(2))]
}

pub fn proto() -> Vec<u8> {
    movie_with(&images(), &[])
}

pub fn pack(proto: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(proto).unwrap();
    encoder.finish().unwrap()
}
