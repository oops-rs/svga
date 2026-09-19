use super::*;
use crate::{
    Compression, ErrorKind,
    fixtures::*,
    movie::{Geometry, LineCap, LineJoin, Rgba, Transform},
};
use std::io::{Cursor, Write};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

const SPEC: &str = r#"{
  "ver": "1.1.0",
  "movie": {"viewBox": {"width": 88, "height": 44.5}, "fps": 20, "frames": 2},
  "images": {"chest": "box", "lost": "nowhere", "odd": 7},
  "sprites": [{
    "imageKey": "chest",
    "matteKey": "",
    "frames": [
      {"alpha": 1, "layout": {"x": 0, "y": 1, "width": 88, "height": 88},
       "transform": {"a": 1.005, "b": 0, "c": 0, "d": 1.005, "tx": -0.22, "ty": 3},
       "clipPath": "M0 0",
       "shapes": [
         {"type": "shape", "args": {"d": "M1 1"}, "styles": {
            "fill": [1, 0.5, 0, 1], "strokeWidth": 2, "lineCap": "round",
            "lineJoin": "bevel", "miterLimit": 4, "lineDash": [1, 2]},
          "transform": {"a": 1, "d": 1}},
         {"type": "rect", "args": {"x": 1, "y": 2, "width": 3, "height": 4, "cornerRadius": 5}},
         {"type": "ellipse", "args": {"x": 1, "y": 2, "radiusX": 3, "radiusY": 4}},
         {"type": "keep"}
       ]},
      {}
    ]
  }]
}"#;

fn archive(files: &[(&str, &[u8])], method: CompressionMethod) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(method);
    writer.add_directory("assets", options).unwrap();
    for (name, bytes) in files {
        writer.start_file(*name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn legacy() -> Vec<u8> {
    // The key `chest` names the file `box.png`; `chest.png` is a decoy.
    let files = [
        ("movie.spec", SPEC.as_bytes()),
        ("box.png", &png(1)[..]),
        ("chest.png", b"decoy"),
    ];
    archive(&files, CompressionMethod::Deflated)
}

fn code(result: Result<Animation>) -> (ErrorKind, &'static str) {
    let error = result.expect_err("expected an error");
    (error.kind(), error.code())
}

#[test]
fn svga_1x_is_read_into_the_same_model() {
    let animation = Animation::from_bytes(&legacy()).unwrap();
    assert_eq!(animation.format(), Format::ZipSpec);
    assert!(animation.document().is_none());
    let movie = animation.movie();
    assert_eq!(movie.version, "1.1.0");
    assert_eq!(movie.params.view_box_height, 44.5);
    assert_eq!((movie.params.fps, movie.params.frames), (20, 2));
    let images: Vec<_> = movie
        .images
        .iter()
        .map(|image| (image.key.as_str(), image.byte_len, image.kind))
        .collect();
    let expected = [
        ("chest", 72, ValueKind::Png),
        ("lost", 7, ValueKind::FileName),
        ("odd", 3, ValueKind::FileName),
    ];
    assert_eq!(images, expected);
    assert_eq!(animation.image("chest"), Some(&png(1)[..]));
    assert_eq!(animation.image("lost"), None);
    let frames = &movie.sprites[0].frames;
    assert_eq!(frames[0].layout.y, 1.0);
    assert_eq!(frames[0].transform.unwrap().a, 1.005);
    assert_eq!(frames[0].clip_path, "M0 0");
    assert_eq!(frames[1], Default::default());
    let shapes = &frames[0].shapes;
    let styles = shapes[0].styles.unwrap();
    let orange = Rgba {
        r: 1.0,
        g: 0.5,
        b: 0.0,
        a: 1.0,
    };
    assert_eq!((styles.fill, styles.stroke), (Some(orange), None));
    assert_eq!(styles.line_cap, LineCap::Round);
    assert_eq!(styles.line_join, LineJoin::Bevel);
    assert_eq!(styles.line_dash, [1.0, 2.0, 0.0]);
    assert_eq!(shapes[0].transform, Some(Transform::IDENTITY));
    assert_eq!(shapes[0].geometry, Geometry::Path { d: "M1 1".into() });
    assert!(
        matches!(shapes[1].geometry, Geometry::Rect { corner_radius, .. } if corner_radius == 5.0)
    );
    assert!(matches!(shapes[2].geometry, Geometry::Ellipse { radius_y, .. } if radius_y == 4.0));
    assert_eq!(shapes[3].geometry, Geometry::Keep);
}

#[test]
fn svga_1x_converts_to_a_self_contained_2x_document() {
    let animation = Animation::from_bytes(&legacy()).unwrap();
    let document = animation.to_document().unwrap();
    // Only images whose file exists can be embedded.
    let keys: Vec<_> = document.images().map(|image| image.key()).collect();
    assert_eq!(keys, [Some("chest")]);
    assert_eq!(document.image("chest"), Some(&png(1)[..]));
    let converted = document.movie().unwrap();
    assert_eq!(converted.sprites, animation.movie().sprites);
    assert_eq!(converted.params, animation.movie().params);
    let reread = Animation::from_bytes(&document.to_bytes(Compression::Fast).unwrap()).unwrap();
    assert_eq!(reread.format(), Format::Zlib);
    assert_eq!(reread.movie(), &converted);
}

#[test]
fn zipped_2x_resolves_images_stored_next_to_the_movie() {
    let images = [
        ("img_0", b"img_0".to_vec()),
        ("img_1", b"gone".to_vec()),
        ("in", png(3)),
    ];
    let proto = movie_with(&images, &[]);
    for binary in [proto.clone(), pack(&proto)] {
        let files = [("movie.binary", &binary[..]), ("img_0.png", &png(1)[..])];
        let bytes = archive(&files, CompressionMethod::Stored);
        let animation = Animation::from_bytes(&bytes).unwrap();
        assert_eq!(animation.format(), Format::ZipBinary);
        assert_eq!(animation.document().unwrap().to_proto(), proto);
        assert_eq!(animation.image("img_0"), Some(&png(1)[..]));
        assert_eq!(animation.image("img_1"), Some(b"gone".as_slice()));
        assert_eq!(animation.image("in"), Some(&png(3)[..]));
        assert_eq!(animation.image("absent"), None);
        let embedded = [
            ("img_0", png(1)),
            ("img_1", b"gone".to_vec()),
            ("in", png(3)),
        ];
        let document = animation.to_document().unwrap();
        assert_eq!(document.to_proto(), movie_with(&embedded, &[]));
    }
}

#[test]
fn embedding_handles_repeated_keys_and_is_bounded() {
    // Two entries share a key and a file: readable, so it must convert too.
    let images = [("a", b"big".to_vec()), ("a", b"big".to_vec())];
    let proto = movie_with(&images, &[]);
    let files = [("movie.binary", &proto[..]), ("big.png", &png(1)[..])];
    let bytes = archive(&files, CompressionMethod::Stored);
    let animation = Animation::from_bytes(&bytes).unwrap();
    let embedded = [("a", png(1)), ("a", png(1))];
    let document = animation.to_document().unwrap();
    assert_eq!(document.to_proto(), movie_with(&embedded, &[]));
    // Many keys naming one file must not multiply past the limit.
    let room = Limits::default().with_max_inflated_bytes(proto.len() + 72);
    let animation = Animation::from_bytes_with(&bytes, &room).unwrap();
    let error = animation.to_document().unwrap_err();
    assert_eq!(error.code(), "svga_inflated_size_exceeds_limit");
    let legacy = Animation::from_bytes_with(
        &legacy(),
        &Limits::default().with_max_inflated_bytes(SPEC.len() + 77),
    );
    assert!(legacy.unwrap().to_document().is_ok());
}

#[test]
fn a_zlib_wrapped_movie_binary_reports_its_own_errors() {
    let proto = proto();
    let trailing = [pack(&proto), vec![0]].concat();
    let bytes = archive(
        &[("movie.binary", &trailing[..])],
        CompressionMethod::Stored,
    );
    let expected = (ErrorKind::Malformed, "svga_trailing_bytes");
    assert_eq!(code(Animation::from_bytes(&bytes)), expected);
    // Inflating shares the budget with the extracted files.
    let packed = pack(&proto);
    let bytes = archive(&[("movie.binary", &packed[..])], CompressionMethod::Stored);
    let tight = Limits::default().with_max_inflated_bytes(packed.len() + proto.len() - 1);
    let expected = (ErrorKind::LimitExceeded, "svga_inflated_size_exceeds_limit");
    assert_eq!(code(Animation::from_bytes_with(&bytes, &tight)), expected);
    let enough = tight.with_max_inflated_bytes(packed.len() + proto.len());
    assert!(Animation::from_bytes_with(&bytes, &enough).is_ok());
}

#[test]
fn a_plain_2x_file_is_read_through_the_same_entry_point() {
    let animation = Animation::from_bytes(&pack(&proto())).unwrap();
    assert_eq!(animation.format(), Format::Zlib);
    assert_eq!(animation.image("img_1"), Some(&png(2)[..]));
    assert_eq!(animation.to_document().unwrap().to_proto(), proto());
    assert_eq!(animation.movie().sprites.len(), 1);
}

#[test]
fn bad_archives_are_refused_without_panicking() {
    let stored = CompressionMethod::Stored;
    let no_movie = archive(&[("chest.png", &png(1)[..])], stored);
    let not_json = archive(&[("movie.spec", b"{")], stored);
    let not_object = archive(&[("movie.spec", b"[]")], stored);
    let odd_shape = br#"{"sprites":[{"frames":[{"shapes":[{"type":"star"}]}]}]}"#;
    let odd_shape = archive(&[("movie.spec", odd_shape)], stored);
    let cases = [
        (no_movie, ErrorKind::Malformed, "svga_zip_without_movie"),
        (not_json, ErrorKind::Malformed, "svga_invalid_spec"),
        (not_object, ErrorKind::Malformed, "svga_invalid_spec"),
        (odd_shape, ErrorKind::Unsupported, "svga_unknown_shape_type"),
        (
            b"PK\x03\x04zip".to_vec(),
            ErrorKind::Malformed,
            "svga_corrupt_zip",
        ),
    ];
    for (bytes, kind, expected) in cases {
        assert_eq!(code(Animation::from_bytes(&bytes)), (kind, expected));
    }
    let whole = legacy();
    for length in 0..whole.len() {
        assert!(Animation::from_bytes(&whole[..length]).is_err());
    }
}

#[test]
fn archive_limits_are_enforced() {
    let bytes = legacy();
    let limited = |limits: Limits| code(Animation::from_bytes_with(&bytes, &limits));
    let exceeded = ErrorKind::LimitExceeded;
    assert_eq!(
        limited(Limits::default().with_max_input_bytes(16)),
        (exceeded, "svga_input_exceeds_limit")
    );
    // The directory entry counts too.
    assert_eq!(
        limited(Limits::default().with_max_archive_entries(3)),
        (exceeded, "svga_too_many_archive_entries")
    );
    assert_eq!(
        limited(Limits::default().with_max_inflated_bytes(SPEC.len() + 76)),
        (exceeded, "svga_inflated_size_exceeds_limit")
    );
    assert_eq!(
        limited(Limits::default().with_max_spec_bytes(SPEC.len() - 1)),
        (exceeded, "svga_spec_exceeds_limit")
    );
    assert_eq!(
        limited(Limits::default().with_max_elements(3)),
        (exceeded, "svga_too_many_elements")
    );
    let exact = Limits::default()
        .with_max_archive_entries(4)
        .with_max_inflated_bytes(SPEC.len() + 77)
        .with_max_spec_bytes(SPEC.len())
        .with_max_elements(10);
    assert!(Animation::from_bytes_with(&bytes, &exact).is_ok());
}
