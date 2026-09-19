use super::*;
use crate::{
    Document, ErrorKind, Limits,
    fixtures::*,
    wire::{float, int32},
};

mod usage;

fn frame(alpha: f32, width: f32, height: f32, transform: Option<Transform>) -> Frame {
    Frame {
        alpha,
        layout: Layout {
            width,
            height,
            ..Layout::default()
        },
        transform,
        ..Frame::default()
    }
}

/// A movie touching every message and field of the schema.
fn full_movie() -> Movie {
    let styles = ShapeStyle {
        fill: Some(Rgba {
            r: 1.0,
            g: 0.5,
            b: 0.25,
            a: 1.0,
        }),
        stroke: Some(Rgba::default()),
        stroke_width: 2.0,
        line_cap: LineCap::Square,
        line_join: LineJoin::Bevel,
        miter_limit: 4.0,
        line_dash: [1.0, 0.0, 3.0],
    };
    let geometries = [
        Geometry::Path {
            d: "M0 0L10 10Z".into(),
        },
        Geometry::Rect {
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
            corner_radius: 0.5,
        },
        Geometry::Ellipse {
            x: -1.0,
            y: -0.0,
            radius_x: f32::MIN_POSITIVE,
            radius_y: f32::MAX,
        },
        Geometry::Keep,
    ];
    let shapes = geometries.into_iter().map(|geometry| Shape {
        geometry,
        styles: Some(styles),
        transform: Some(Transform::IDENTITY),
    });
    let drawn = Frame {
        clip_path: "M0 0".into(),
        shapes: shapes.collect(),
        layout: Layout {
            x: 5.0,
            y: -6.0,
            width: 64.0,
            height: 32.0,
        },
        ..frame(0.75, 0.0, 0.0, Some(Transform::default()))
    };
    Movie {
        version: "2.1.0".into(),
        params: Params {
            view_box_width: 750.0,
            view_box_height: 1334.0,
            fps: 30,
            frames: 2,
        },
        images: Vec::new(),
        sprites: vec![
            Sprite {
                image_key: "hat".into(),
                frames: vec![drawn, Frame::default()],
                matte_key: "mask.matte".into(),
            },
            Sprite::default(),
        ],
        audios: vec![Audio {
            audio_key: "song".into(),
            start_frame: 1,
            end_frame: i32::MAX,
            start_time: -1,
            total_time: i32::MIN,
        }],
    }
}

#[test]
fn every_part_of_the_schema_survives_encode_and_decode() {
    let movie = full_movie();
    let images = [("hat", png(1)), ("song", b"ID3".to_vec())];
    let borrowed: Vec<(&str, &[u8])> = images.iter().map(|(k, v)| (*k, v.as_slice())).collect();
    let document = Document::from_movie(&movie, &borrowed).unwrap();
    let decoded = document.movie().unwrap();
    let expected = Movie {
        images: vec![
            ImageInfo {
                key: "hat".into(),
                byte_len: 72,
                kind: ValueKind::Png,
            },
            ImageInfo {
                key: "song".into(),
                byte_len: 3,
                kind: ValueKind::Mp3,
            },
        ],
        ..movie
    };
    assert_eq!(decoded, expected);
    // -0.0 is not a default value and must be stored.
    let Geometry::Ellipse { y, .. } = decoded.sprites[0].frames[0].shapes[2].geometry else {
        panic!("expected an ellipse");
    };
    assert!(y.is_sign_negative());
    // Encoding is deterministic.
    let again = Document::from_movie(&decoded, &borrowed).unwrap();
    assert_eq!(again.to_proto(), document.to_proto());
}

#[test]
fn a_hand_encoded_movie_is_read_and_unknown_fields_are_skipped() {
    let movie = Document::from_proto(proto()).unwrap().movie().unwrap();
    assert_eq!(movie.version, "2.1.0");
    let expected = Params {
        view_box_width: 64.0,
        view_box_height: 48.0,
        fps: 20,
        frames: 2,
    };
    assert_eq!(movie.params, expected);
    assert_eq!(movie.images.len(), 2);
    let expected = Sprite {
        image_key: "img_0".into(),
        frames: vec![frame(1.0, 64.0, 64.0, None); 2],
        matte_key: String::new(),
    };
    assert_eq!(movie.sprites, [expected]);
    assert!(movie.audios.is_empty());
}

#[test]
fn a_stored_transform_starts_from_zero_and_a_missing_one_is_the_identity() {
    let partial = field(3, &[float(1, 2.0), float(6, 9.0)].concat());
    let sprite = field(4, &[field(2, &partial), field(2, &[])].concat());
    let movie = Document::from_proto(sprite).unwrap().movie().unwrap();
    let frames = &movie.sprites[0].frames;
    let expected = Transform {
        a: 2.0,
        ty: 9.0,
        ..Transform::default()
    };
    assert_eq!(frames[0].transform, Some(expected));
    assert_eq!(frames[1].transform, None);
    assert_eq!(frames[1].matrix(), Transform::IDENTITY);
    assert!(!frames[1].is_visible());
}

#[test]
fn repeated_messages_merge_and_repeated_scalars_keep_the_last() {
    let proto = [
        field(1, b"1.0.0"),
        field(2, &[float(1, 10.0), int32(3, 5)].concat()),
        field(1, b"2.0.0"),
        field(2, &[int32(3, 60), int32(3, 24)].concat()),
        image_entry("k", b"first"),
        image_entry("k", &png(1)),
    ]
    .concat();
    let movie = Document::from_proto(proto).unwrap().movie().unwrap();
    assert_eq!(movie.version, "2.0.0");
    assert_eq!((movie.params.view_box_width, movie.params.fps), (10.0, 24));
    let expected = ImageInfo {
        key: "k".into(),
        byte_len: 72,
        kind: ValueKind::Png,
    };
    assert_eq!(movie.images, [expected]);
}

#[test]
fn a_negative_zero_layout_and_lossy_keys_are_handled() {
    let mut movie = full_movie();
    movie.sprites[0].frames[1].layout.x = -0.0;
    let decoded = Document::from_movie(&movie, &[]).unwrap().movie().unwrap();
    assert!(decoded.sprites[0].frames[1].layout.x.is_sign_negative());
    // Two different invalid keys read as the same lossy key: listed once.
    let entry = |key: &[u8]| field(3, &[field(1, key), field(2, b"v")].concat());
    let proto = [entry(&[0xff]), entry(&[0xfe])].concat();
    let movie = Document::from_proto(proto).unwrap().movie().unwrap();
    assert_eq!(movie.images.len(), 1);
}

#[test]
fn invalid_nested_data_is_reported_not_guessed() {
    let shape = |kind: i32| field(4, &field(2, &field(5, &int32(1, kind))));
    let styles = |number, value| field(4, &field(2, &field(5, &field(10, &int32(number, value)))));
    let cases = [
        (shape(4), ErrorKind::Unsupported, "svga_unknown_shape_type"),
        (shape(-1), ErrorKind::Unsupported, "svga_unknown_shape_type"),
        (
            styles(4, 3),
            ErrorKind::Unsupported,
            "svga_unknown_line_cap",
        ),
        (
            styles(5, 3),
            ErrorKind::Unsupported,
            "svga_unknown_line_join",
        ),
        (field(1, &[0xff]), ErrorKind::Malformed, "svga_invalid_utf8"),
        (
            field(2, &int32(1, 7)),
            ErrorKind::Malformed,
            "svga_unexpected_wire_type",
        ),
        (
            field(2, &float(3, 7.0)),
            ErrorKind::Malformed,
            "svga_unexpected_wire_type",
        ),
        (
            field(4, &field(2, &[0x0d, 0, 0])),
            ErrorKind::Malformed,
            "svga_truncated_field",
        ),
        (
            field(5, &field(1, &[0xc0])),
            ErrorKind::Malformed,
            "svga_invalid_utf8",
        ),
    ];
    for (proto, kind, code) in cases {
        let error = Document::from_proto(proto).unwrap().movie().unwrap_err();
        assert_eq!((error.kind(), error.code()), (kind, code));
    }
}

#[test]
fn the_header_frame_count_only_hints_and_cannot_force_an_allocation() {
    let sprite = field(4, &[field(2, &[]), field(2, &[])].concat());
    for frames in [i32::MAX, -1, 0, 2] {
        let params = field(2, &int32(4, frames));
        let proto = [params, sprite.clone()].concat();
        let movie = Document::from_proto(proto).unwrap().movie().unwrap();
        assert_eq!(movie.sprites[0].frames.len(), 2);
        // Never more than the sprite's own bytes could hold, whatever the header says.
        assert!(movie.sprites[0].frames.capacity() <= 4, "{frames}");
    }
    // Nor more than the budget allows.
    let params = field(2, &int32(4, 1000));
    let frames: Vec<u8> = (0..1000).flat_map(|_| field(2, &[])).collect();
    let document = Document::from_proto([params, field(4, &frames)].concat()).unwrap();
    let error = document
        .movie_with(&Limits::default().with_max_elements(10))
        .unwrap_err();
    assert_eq!(error.code(), "svga_too_many_elements");
    // A sprite stored before the params simply gets no hint.
    let early = [sprite.clone(), field(2, &int32(4, 2))].concat();
    let movie = Document::from_proto(early).unwrap().movie().unwrap();
    assert_eq!((movie.sprites[0].frames.len(), movie.params.frames), (2, 2));
}

#[test]
fn the_element_budget_bounds_the_typed_view() {
    // Two images, one sprite with two frames: five elements.
    let document = Document::from_proto(proto()).unwrap();
    let enough = Limits::default().with_max_elements(5);
    assert!(document.movie_with(&enough).is_ok());
    let short = Limits::default().with_max_elements(4);
    let error = document.movie_with(&short).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::LimitExceeded);
    assert_eq!(error.code(), "svga_too_many_elements");
    assert_eq!(error.to_string(), "svga_too_many_elements");
}
