use super::*;

fn info(key: &str) -> ImageInfo {
    ImageInfo {
        key: key.into(),
        byte_len: 0,
        kind: ValueKind::Png,
    }
}

fn sprite(image_key: &str, matte_key: &str, frames: Vec<Frame>) -> Sprite {
    Sprite {
        image_key: image_key.into(),
        frames,
        matte_key: matte_key.into(),
    }
}

fn scaled(x: f32, y: f32) -> Option<Transform> {
    Some(Transform {
        a: x,
        d: y,
        ..Transform::default()
    })
}

fn movie() -> Movie {
    let quarter_turn = Some(Transform {
        b: 2.0,
        c: -2.0,
        ..Transform::default()
    });
    Movie {
        images: ["hat", "mask", "song", "spare"].map(info).into(),
        sprites: vec![
            sprite(
                "hat",
                "mask.matte",
                vec![
                    frame(1.0, 100.0, 50.0, None),
                    frame(0.5, 100.0, 50.0, scaled(0.5, 3.0)),
                    // Invisible and non-finite frames do not count.
                    frame(0.0, 100.0, 50.0, scaled(100.0, 100.0)),
                    frame(1.0, f32::INFINITY, 50.0, None),
                ],
            ),
            sprite("hat", "", vec![frame(1.0, 10.0, 10.0, quarter_turn)]),
            sprite("mask.matte", "", vec![frame(1.0, -8.0, 8.0, None)]),
            sprite("ghost", "", vec![]),
        ],
        audios: vec![Audio {
            audio_key: "song".into(),
            ..Audio::default()
        }],
        ..Movie::default()
    }
}

#[test]
fn keys_are_resolved_through_sprites_mattes_and_audio() {
    let movie = movie();
    let referenced: Vec<_> = movie.referenced_keys().into_iter().collect();
    assert_eq!(referenced, ["ghost", "hat", "mask", "song"]);
    assert_eq!(movie.unused_image_keys(), ["spare"]);
    assert_eq!(movie.missing_image_keys(), ["ghost"]);
    assert!(movie.sprites[2].is_matte() && !movie.sprites[0].is_matte());
    assert_eq!(movie.sprites[2].image_name(), "mask");
}

#[test]
fn the_largest_drawn_size_is_taken_per_axis_over_visible_frames() {
    let usage = movie().image_usage();
    let hat = ImageUsage {
        key: "hat".into(),
        sprites: 2,
        visible_frames: 4,
        // Width from the unscaled frame, height from the 3x one; the rotated
        // 10x10 sprite stays 20x20.
        max_drawn_size: Some(DrawnSize {
            width: 100.0,
            height: 150.0,
        }),
    };
    assert_eq!(usage[0], hat);
    let mask = DrawnSize {
        width: 8.0,
        height: 8.0,
    };
    assert_eq!((usage[1].sprites, usage[1].max_drawn_size), (1, Some(mask)));
    for unused in &usage[2..] {
        assert_eq!((unused.sprites, unused.max_drawn_size), (0, None));
    }
    assert_eq!(Transform::IDENTITY.scale(), (1.0, 1.0));
}
