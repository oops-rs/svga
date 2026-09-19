//! The crate as a user drives it: build, write, read, inspect, edit, write.
use svga::{
    Animation, Compression, Document, ErrorKind, Format, Movie, ValueKind,
    movie::{Frame, Layout, Params, Sprite},
};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nnot decoded by this crate";

fn movie() -> Movie {
    let frame = Frame {
        alpha: 1.0,
        layout: Layout {
            width: 40.0,
            height: 20.0,
            ..Layout::default()
        },
        ..Frame::default()
    };
    Movie {
        version: "2.0.0".into(),
        params: Params {
            view_box_width: 100.0,
            view_box_height: 100.0,
            fps: 24,
            frames: 1,
        },
        sprites: vec![Sprite {
            image_key: "hat".into(),
            frames: vec![frame],
            ..Sprite::default()
        }],
        ..Movie::default()
    }
}

#[test]
fn a_file_is_built_read_edited_and_read_again() {
    let images: [(&str, &[u8]); 2] = [("hat", PNG), ("spare", PNG)];
    let file = Document::from_movie(&movie(), &images)
        .unwrap()
        .to_bytes(Compression::Default)
        .unwrap();

    let animation = Animation::from_bytes(&file).unwrap();
    assert_eq!(animation.format(), Format::Zlib);
    assert_eq!(animation.movie().sprites, movie().sprites);
    assert_eq!(animation.movie().unused_image_keys(), ["spare"]);
    let usage = &animation.movie().image_usage()[0];
    let drawn = usage.max_drawn_size.unwrap();
    assert_eq!((drawn.width, drawn.height), (40.0, 20.0));

    let document = animation.document().unwrap();
    let kinds: Vec<_> = document.images().map(|image| image.kind()).collect();
    assert_eq!(kinds, [ValueKind::Png, ValueKind::Png]);
    let edited = document
        .remove_image("spare")
        .and_then(|document| document.replace_image("hat", b"tiny"))
        .unwrap();
    assert!(edited.proto_len() < document.proto_len());
    let error = edited.remove_image("spare").unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidEdit);

    let reread = Document::from_bytes(&edited.to_bytes(Compression::Best).unwrap()).unwrap();
    assert_eq!(reread.image("hat"), Some(b"tiny".as_slice()));
    assert_eq!(reread.movie().unwrap().sprites, movie().sprites);
    // Everything but the two edited entries is byte-identical.
    let untouched = |document: &Document| -> Vec<Vec<u8>> {
        let others = document.fields().filter(|field| field.number != 3);
        others.map(|field| field.raw.to_vec()).collect()
    };
    assert_eq!(untouched(&reread), untouched(document));
}
