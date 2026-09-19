//! [`Movie`] → protobuf, in canonical proto3 form: fields in number order,
//! zero-valued scalars and empty strings left out.
//!
//! This builds new documents (converting SVGA 1.x, fixtures). It is not how
//! an existing file is edited: that goes through [`Document`](crate::Document).
use super::{
    Audio, Frame, Geometry, Layout, Movie, Params, Rgba, Shape, ShapeStyle, Sprite, Transform,
};
use crate::wire;

fn float(number: u32, value: f32) -> Vec<u8> {
    // Compare bits so that -0.0 and NaN payloads are kept.
    if value.to_bits() == 0 {
        Vec::new()
    } else {
        wire::float(number, value)
    }
}

fn floats(values: &[f32]) -> Vec<u8> {
    (1..).zip(values).flat_map(|(n, v)| float(n, *v)).collect()
}

fn int32(number: u32, value: i32) -> Vec<u8> {
    if value == 0 {
        Vec::new()
    } else {
        wire::int32(number, value)
    }
}

fn string(number: u32, value: &str) -> Vec<u8> {
    if value.is_empty() {
        Vec::new()
    } else {
        wire::length_delimited(number, value.as_bytes())
    }
}

fn message(number: u32, payload: &[u8]) -> Vec<u8> {
    wire::length_delimited(number, payload)
}

/// The protobuf `MovieEntity` for `movie`, embedding `images` in order.
pub(crate) fn movie(movie: &Movie, images: &[(&str, &[u8])]) -> Vec<u8> {
    let entries = images.iter().flat_map(|(key, value)| {
        let entry = [
            wire::length_delimited(1, key.as_bytes()),
            wire::length_delimited(2, value),
        ];
        message(3, &entry.concat())
    });
    [
        string(1, &movie.version),
        message(2, &params(&movie.params)),
        entries.collect(),
        movie
            .sprites
            .iter()
            .flat_map(|item| message(4, &sprite(item)))
            .collect(),
        movie
            .audios
            .iter()
            .flat_map(|item| message(5, &audio(item)))
            .collect(),
    ]
    .concat()
}

fn params(params: &Params) -> Vec<u8> {
    [
        float(1, params.view_box_width),
        float(2, params.view_box_height),
        int32(3, params.fps),
        int32(4, params.frames),
    ]
    .concat()
}

fn sprite(sprite: &Sprite) -> Vec<u8> {
    let frames = sprite
        .frames
        .iter()
        .flat_map(|item| message(2, &frame(item)));
    [
        string(1, &sprite.image_key),
        frames.collect(),
        string(3, &sprite.matte_key),
    ]
    .concat()
}

fn frame(frame: &Frame) -> Vec<u8> {
    let layout = if frame.layout == Layout::default() {
        Vec::new()
    } else {
        let Layout {
            x,
            y,
            width,
            height,
        } = frame.layout;
        message(2, &floats(&[x, y, width, height]))
    };
    [
        float(1, frame.alpha),
        layout,
        frame
            .transform
            .map(|m| transform(3, &m))
            .unwrap_or_default(),
        string(4, &frame.clip_path),
        frame
            .shapes
            .iter()
            .flat_map(|item| message(5, &shape(item)))
            .collect(),
    ]
    .concat()
}

fn transform(number: u32, matrix: &Transform) -> Vec<u8> {
    let Transform { a, b, c, d, tx, ty } = *matrix;
    message(number, &floats(&[a, b, c, d, tx, ty]))
}

fn shape(shape: &Shape) -> Vec<u8> {
    let (kind, number, arguments) = match &shape.geometry {
        Geometry::Path { d } => (0, 2, string(1, d)),
        Geometry::Rect {
            x,
            y,
            width,
            height,
            corner_radius,
        } => (1, 3, floats(&[*x, *y, *width, *height, *corner_radius])),
        Geometry::Ellipse {
            x,
            y,
            radius_x,
            radius_y,
        } => (2, 4, floats(&[*x, *y, *radius_x, *radius_y])),
        Geometry::Keep => (3, 0, Vec::new()),
    };
    let arguments = if number == 0 {
        Vec::new()
    } else {
        message(number, &arguments)
    };
    [
        int32(1, kind),
        arguments,
        shape.styles.map(|s| styles(&s)).unwrap_or_default(),
        shape
            .transform
            .map(|m| transform(11, &m))
            .unwrap_or_default(),
    ]
    .concat()
}

fn color(number: u32, color: Option<Rgba>) -> Vec<u8> {
    color
        .map(|Rgba { r, g, b, a }| message(number, &floats(&[r, g, b, a])))
        .unwrap_or_default()
}

fn styles(style: &ShapeStyle) -> Vec<u8> {
    let [dash_one, dash_two, dash_three] = style.line_dash;
    let payload = [
        color(1, style.fill),
        color(2, style.stroke),
        float(3, style.stroke_width),
        int32(4, style.line_cap as i32),
        int32(5, style.line_join as i32),
        float(6, style.miter_limit),
        float(7, dash_one),
        float(8, dash_two),
        float(9, dash_three),
    ]
    .concat();
    message(10, &payload)
}

fn audio(audio: &Audio) -> Vec<u8> {
    [
        string(1, &audio.audio_key),
        int32(2, audio.start_frame),
        int32(3, audio.end_frame),
        int32(4, audio.start_time),
        int32(5, audio.total_time),
    ]
    .concat()
}
