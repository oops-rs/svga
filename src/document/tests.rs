use super::*;
use crate::{Container, ErrorKind, fixtures::*, wire::float};

mod edits;

fn code(result: Result<Document>) -> (ErrorKind, &'static str) {
    let error = result.expect_err("expected an error");
    (error.kind(), error.code())
}

#[test]
fn an_unedited_document_re_emits_its_exact_payload() {
    // Out-of-order fields, an unknown top-level varint, a NaN with a payload
    // and a value-before-key entry: none of it is canonical, all of it stays.
    let odd_entry = field(
        3,
        &[field(2, &png(9)), field(7, b"x"), field(1, b"odd")].concat(),
    );
    let nan = float(1, f32::from_bits(0x7fc0_1234));
    let proto = [
        sprite("img_0", 0.5),
        odd_entry,
        crate::wire::int32(77, -1),
        header(),
        field(2, &nan),
        proto(),
    ]
    .concat();
    let document = Document::from_bytes(&pack(&proto)).unwrap();
    assert_eq!(document.to_proto(), proto);
    assert_eq!(document.proto_len(), proto.len());
    assert!(format!("{document:?}").len() < 1024);
    assert!(document.has_unknown_fields());
    for compression in [Compression::Fast, Compression::Default, Compression::Best] {
        let bytes = document.to_bytes(compression).unwrap();
        assert_eq!(Document::from_bytes(&bytes).unwrap().to_proto(), proto);
    }
}

#[test]
fn images_are_listed_in_stored_order_with_their_kind() {
    let images = [
        ("img_0", png(1)),
        ("name", b"chest.png".to_vec()),
        ("audio", vec![0xff, 0xfb, 0x90, 0x00]),
        ("tagged", b"ID3\x04rest".to_vec()),
        ("blob", vec![0, 159, 146, 150]),
    ];
    let document = Document::from_proto(movie_with(&images, &[])).unwrap();
    let listed: Vec<_> = document
        .images()
        .map(|image| (image.key().unwrap(), image.value().len(), image.kind()))
        .collect();
    assert_eq!(
        listed,
        [
            ("img_0", 72, ValueKind::Png),
            ("name", 9, ValueKind::FileName),
            ("audio", 4, ValueKind::Mp3),
            ("tagged", 8, ValueKind::Mp3),
            ("blob", 4, ValueKind::Unknown),
        ]
    );
    assert!(document.images().all(|image| image.is_canonical()));
    assert_eq!(document.image("name"), Some(b"chest.png".as_slice()));
    assert_eq!(document.image("absent"), None);
    assert_eq!(document.version(), Some("2.1.0"));
    assert!(!document.has_unknown_fields());
}

#[test]
fn animated_pngs_are_told_apart() {
    assert!(is_animated_png(&animated_png()));
    assert!(!is_animated_png(&still_png()));
    assert!(!is_animated_png(&png(1)));
    assert!(!is_animated_png(b"acTL"));
    let huge_chunk = [PNG_SIGNATURE, &[0xff; 8]].concat();
    assert!(!is_animated_png(&huge_chunk));
    assert_eq!(png_dimensions(&still_png()), Some((256, 64)));
    // Not a PNG, no IHDR first, or cut short inside the header.
    assert_eq!(png_dimensions(&png(1)), None);
    assert_eq!(png_dimensions(b"IHDR"), None);
    let whole = still_png();
    assert!((0..24).all(|length| png_dimensions(&whole[..length]).is_none()));
    assert_eq!(ValueKind::sniff(&[0xff, 0xd8, 0xff, 0xe0]), ValueKind::Jpeg);
}

#[test]
fn map_entries_follow_protobuf_semantics() {
    let repeated = [
        field(1, b"first"),
        field(2, b"one"),
        field(1, b"key"),
        field(2, b"two"),
    ]
    .concat();
    let proto = [
        header(),
        field(3, &repeated),
        field(3, &[]),
        image_entry("key", b"three"),
    ]
    .concat();
    let document = Document::from_proto(proto).unwrap();
    let entries: Vec<_> = document.images().collect();
    assert_eq!(entries[0].key(), Some("key"));
    assert_eq!(entries[0].value(), b"two");
    assert_eq!(entries[0].field_numbers(), [1, 2, 1, 2]);
    assert!(!entries[0].is_canonical());
    assert_eq!((entries[1].key(), entries[1].value()), (Some(""), &[][..]));
    // An empty value names no file.
    assert_eq!(entries[1].kind(), ValueKind::Unknown);
    // The last entry for a key wins.
    assert_eq!(document.image("key"), Some(b"three".as_slice()));
    let invalid = Document::from_proto(
        image_entry("k", b"v")
            .iter()
            .map(|b| if *b == b'k' { 0xff } else { *b })
            .collect::<Vec<u8>>(),
    )
    .unwrap();
    assert_eq!(invalid.images().next().unwrap().key(), None);
}

#[test]
fn malformed_input_is_rejected_without_panicking() {
    let cases = [
        (
            pack(&[header(), vec![0x22, 0x80]].concat()),
            "svga_truncated_varint",
        ),
        (
            pack(&[header(), vec![0x22, 0x7f, 1, 2, 3]].concat()),
            "svga_truncated_field",
        ),
        (
            pack(&[header(), vec![0xff; 11]].concat()),
            "svga_varint_overflow",
        ),
        (
            pack(&[header(), vec![0x23]].concat()),
            "svga_unsupported_wire_type",
        ),
        (
            pack(&[header(), vec![0x00, 0x00]].concat()),
            "svga_invalid_field_number",
        ),
        (
            pack(&[header(), vec![0x20, 0x01]].concat()),
            "svga_unexpected_wire_type",
        ),
        (pack(&field(3, &[0x08, 0x01])), "svga_unexpected_wire_type"),
        (pack(&field(3, &[0x0a, 0x05, 1])), "svga_truncated_field"),
        (b"definitely not an svga file".to_vec(), "svga_not_zlib"),
        (Vec::new(), "svga_not_zlib"),
        (vec![0x78], "svga_not_zlib"),
        (vec![0x78, 0x9c, 1, 2, 3, 4], "svga_corrupt_zlib_stream"),
        ([pack(&proto()), vec![0]].concat(), "svga_trailing_bytes"),
    ];
    for (bytes, expected) in cases {
        assert_eq!(
            code(Document::from_bytes(&bytes)),
            (ErrorKind::Malformed, expected)
        );
    }
    let whole = pack(&proto());
    for length in 0..whole.len() {
        assert!(Document::from_bytes(&whole[..length]).is_err());
    }
}

#[test]
fn every_prefix_of_a_payload_is_handled_without_panicking() {
    let whole = proto();
    for length in 0..whole.len() {
        if let Ok(document) = Document::from_proto(&whole[..length]) {
            assert_eq!(document.to_proto(), &whole[..length]);
            let _ = document.movie();
        }
    }
}

#[test]
fn limits_are_enforced_and_configurable() {
    let bomb = pack(&vec![0; 1024 * 1024]);
    assert!(bomb.len() < 8 * 1024);
    let small = Limits::default().with_max_inflated_bytes(64 * 1024);
    assert_eq!(
        code(Document::from_bytes_with(&bomb, &small)),
        (ErrorKind::LimitExceeded, "svga_inflated_size_exceeds_limit")
    );
    let exact = Limits::default().with_max_inflated_bytes(proto().len());
    assert!(Document::from_bytes_with(&pack(&proto()), &exact).is_ok());
    let tiny = Limits::default().with_max_input_bytes(8);
    assert_eq!(
        code(Document::from_bytes_with(&pack(&proto()), &tiny)),
        (ErrorKind::LimitExceeded, "svga_input_exceeds_limit")
    );
    // Tiny fields cost far more memory than their two bytes; they are counted.
    let many = [0x30, 0x00].repeat(100);
    let fields = Limits::default().with_max_fields(99);
    assert_eq!(
        code(Document::from_proto_with(many.clone(), &fields)),
        (ErrorKind::LimitExceeded, "svga_too_many_fields")
    );
    assert!(Document::from_proto_with(many.clone(), &fields.with_max_fields(100)).is_ok());
    let bytes = Limits::default().with_max_inflated_bytes(199);
    assert_eq!(
        code(Document::from_proto_with(many.clone(), &bytes)),
        (ErrorKind::LimitExceeded, "svga_inflated_size_exceeds_limit")
    );
    assert_eq!(
        code(Document::from_bytes_with(&pack(&many), &fields)),
        (ErrorKind::LimitExceeded, "svga_too_many_fields")
    );
    assert_eq!(
        code(Document::from_bytes(b"PK\x03\x04zip")),
        (ErrorKind::Unsupported, "svga_zip_container")
    );
    assert_eq!(Container::detect(b"PK\x03\x04"), Some(Container::Zip));
    assert_eq!(Container::detect(&pack(&[])), Some(Container::Zlib));
    assert_eq!(Container::detect(b"??"), None);
}
