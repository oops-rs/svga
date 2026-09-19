use super::*;

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[test]
fn replacing_an_image_leaves_every_other_byte_alone() {
    let original = Document::from_proto(proto()).unwrap();
    let edited = original.replace_image("img_1", b"smaller").unwrap();
    let expected = movie_with(&[("img_0", png(1)), ("img_1", b"smaller".to_vec())], &[]);
    assert_eq!(edited.to_proto(), expected);
    assert_eq!(edited.image("img_1"), Some(b"smaller".as_slice()));
    // Unknown nested fields ride along, and the receiver is unchanged.
    assert!(contains(&edited.to_proto(), &sprite("img_0", 1.0)));
    assert_eq!(original.to_proto(), proto());
    // Putting the old value back restores the exact payload.
    let restored = edited.replace_image("img_1", &png(2)).unwrap();
    assert_eq!(restored.to_proto(), proto());
}

#[test]
fn replacing_keeps_entry_order_and_unknown_entry_fields() {
    let entry = [field(2, b"old"), field(7, b"extra"), field(1, b"odd")].concat();
    let document = Document::from_proto([header(), field(3, &entry)].concat()).unwrap();
    let edited = document.replace_image("odd", b"new").unwrap();
    let expected = [field(2, b"new"), field(7, b"extra"), field(1, b"odd")].concat();
    assert_eq!(edited.to_proto(), [header(), field(3, &expected)].concat());
    assert_eq!(edited.images().next().unwrap().field_numbers(), [2, 7, 1]);
    // An entry without a value gains one; with several, the winner changes.
    let bare = Document::from_proto(field(3, &field(1, b"k"))).unwrap();
    let filled = bare.replace_image("k", b"v").unwrap();
    assert_eq!(filled.to_proto(), image_entry("k", b"v"));
    let twice = [field(1, b"k"), field(2, b"a"), field(2, b"b")].concat();
    let document = Document::from_proto(field(3, &twice)).unwrap();
    let edited = document.replace_image("k", b"c").unwrap();
    let expected = [field(1, b"k"), field(2, b"a"), field(2, b"c")].concat();
    assert_eq!(edited.to_proto(), field(3, &expected));
}

#[test]
fn images_can_be_removed_and_inserted() {
    let document = Document::from_proto(proto()).unwrap();
    let removed = document.remove_image("img_0").unwrap();
    assert_eq!(removed.to_proto(), movie_with(&[("img_1", png(2))], &[]));
    let inserted = document.insert_image("img_2", &png(3)).unwrap();
    let mut all = images();
    all.push(("img_2", png(3)));
    assert_eq!(inserted.to_proto(), movie_with(&all, &[]));
    // Without any image the entry goes where field 3 belongs.
    let empty = Document::from_proto(movie_with(&[], &[])).unwrap();
    let first = empty.insert_image("img_0", &png(1)).unwrap();
    assert_eq!(first.to_proto(), movie_with(&[("img_0", png(1))], &[]));
    let only_header = Document::from_proto(header()).unwrap();
    let appended = only_header.insert_image("k", b"v").unwrap();
    assert_eq!(
        appended.to_proto(),
        [header(), image_entry("k", b"v")].concat()
    );
    // Edited documents read back like any other.
    let reread = Document::from_bytes(&inserted.to_bytes(Compression::Fast).unwrap()).unwrap();
    assert_eq!(reread.images().count(), 3);
}

#[test]
fn edits_that_cannot_apply_are_refused() {
    let duplicated = movie_with(&[("img_0", png(1)), ("img_0", png(2))], &[]);
    let duplicated = Document::from_proto(duplicated).unwrap();
    let document = Document::from_proto(proto()).unwrap();
    let cases = [
        (
            document.replace_image("absent", b""),
            "svga_image_key_not_found",
        ),
        (document.remove_image("absent"), "svga_image_key_not_found"),
        (document.insert_image("img_0", b""), "svga_image_key_exists"),
        (
            duplicated.replace_image("img_0", b""),
            "svga_image_key_ambiguous",
        ),
        (duplicated.remove_image("img_0"), "svga_image_key_ambiguous"),
        (
            duplicated.insert_image("img_0", b""),
            "svga_image_key_exists",
        ),
    ];
    for (result, expected) in cases {
        assert_eq!(code(result), (ErrorKind::InvalidEdit, expected));
    }
}
