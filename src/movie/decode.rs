//! Protobuf → [`Movie`]. Unknown fields are skipped, a known field with the
//! wrong wire type is malformed, and repeated scalars follow last-one-wins.
//! Repeated params, layouts and transforms merge as protobuf says; a repeated
//! shape argument or style message replaces the earlier one.
use super::{Audio, Frame, ImageInfo, Layout, Movie, Params, Sprite, Transform, decode_shape};
use crate::{
    Document, Limits,
    document::{AUDIOS, PARAMS, SPRITES, VERSION},
    error::{Error, Result},
    wire::{self, Field},
};
use std::{cell::Cell, collections::HashMap};

/// Counts decoded elements so a small file cannot expand without bound.
pub(super) struct Budget(Cell<usize>);

impl Budget {
    pub fn spend(&self) -> Result<()> {
        let left = self.0.get().checked_sub(1);
        self.0
            .set(left.ok_or(Error::limit("svga_too_many_elements"))?);
        Ok(())
    }
}

/// Fold the fields of one message into `initial`.
pub(super) fn fold<'a, T>(
    payload: &'a [u8],
    initial: T,
    mut apply: impl FnMut(T, Field<'a>) -> Result<T>,
) -> Result<T> {
    wire::fields(payload).try_fold(initial, |value, field| apply(value, field?))
}

pub(crate) fn movie(document: &Document, limits: &Limits) -> Result<Movie> {
    let budget = Budget(Cell::new(limits.max_elements));
    let header = Movie {
        images: images(document, &budget)?,
        ..Movie::default()
    };
    document.fields().try_fold(header, |movie, field| {
        Ok(match field.number {
            VERSION => Movie {
                version: field.str()?.to_owned(),
                ..movie
            },
            PARAMS => Movie {
                params: params(movie.params, field.bytes()?)?,
                ..movie
            },
            SPRITES => {
                let sprite = sprite(field.bytes()?, &budget)?;
                Movie {
                    sprites: pushed(movie.sprites, sprite),
                    ..movie
                }
            }
            AUDIOS => {
                let audio = audio(field.bytes()?, &budget)?;
                Movie {
                    audios: pushed(movie.audios, audio),
                    ..movie
                }
            }
            _ => movie,
        })
    })
}

pub(super) fn pushed<T>(mut items: Vec<T>, item: T) -> Vec<T> {
    items.push(item);
    items
}

/// Distinct keys in first-seen order, each described by its winning value.
/// Entries count against the element budget like everything else decoded.
fn images(document: &Document, budget: &Budget) -> Result<Vec<ImageInfo>> {
    // Keyed by the exposed (lossy) key, so `Movie::images` never repeats one.
    let mut slots: HashMap<String, usize> = HashMap::new();
    let mut infos: Vec<ImageInfo> = Vec::new();
    for image in document.images() {
        budget.spend()?;
        let info = ImageInfo {
            key: String::from_utf8_lossy(image.key_bytes()).into_owned(),
            byte_len: image.value().len(),
            kind: image.kind(),
        };
        let slot = *slots.entry(info.key.clone()).or_insert(infos.len());
        match infos.get_mut(slot) {
            Some(seen) => *seen = info,
            None => infos.push(info),
        }
    }
    Ok(infos)
}

fn params(initial: Params, payload: &[u8]) -> Result<Params> {
    fold(payload, initial, |params, field| {
        Ok(match field.number {
            1 => Params {
                view_box_width: field.f32()?,
                ..params
            },
            2 => Params {
                view_box_height: field.f32()?,
                ..params
            },
            3 => Params {
                fps: field.i32()?,
                ..params
            },
            4 => Params {
                frames: field.i32()?,
                ..params
            },
            _ => params,
        })
    })
}

fn sprite(payload: &[u8], budget: &Budget) -> Result<Sprite> {
    budget.spend()?;
    // These decoders run once per frame of every sprite, so they fill one
    // local value in place rather than rebuilding it for each field.
    let mut sprite = Sprite::default();
    for field in wire::fields(payload) {
        let field = field?;
        match field.number {
            1 => sprite.image_key = field.str()?.to_owned(),
            2 => sprite.frames.push(frame(field.bytes()?, budget)?),
            3 => sprite.matte_key = field.str()?.to_owned(),
            _ => {}
        }
    }
    Ok(sprite)
}

fn frame(payload: &[u8], budget: &Budget) -> Result<Frame> {
    budget.spend()?;
    let mut frame = Frame::default();
    for field in wire::fields(payload) {
        let field = field?;
        match field.number {
            1 => frame.alpha = field.f32()?,
            2 => frame.layout = layout(frame.layout, field.bytes()?)?,
            3 => frame.transform = Some(transform(frame.transform, field.bytes()?)?),
            4 => frame.clip_path = field.str()?.to_owned(),
            5 => frame
                .shapes
                .push(decode_shape::shape(field.bytes()?, budget)?),
            _ => {}
        }
    }
    Ok(frame)
}

fn layout(initial: Layout, payload: &[u8]) -> Result<Layout> {
    let mut layout = initial;
    for field in wire::fields(payload) {
        let field = field?;
        match field.number {
            1 => layout.x = field.f32()?,
            2 => layout.y = field.f32()?,
            3 => layout.width = field.f32()?,
            4 => layout.height = field.f32()?,
            _ => {}
        }
    }
    Ok(layout)
}

/// A transform that is present starts from all zeros, not from the identity:
/// protobuf omits zero-valued floats, so `{a: 1, d: 1}` stores only two fields.
pub(super) fn transform(initial: Option<Transform>, payload: &[u8]) -> Result<Transform> {
    let mut matrix = initial.unwrap_or_default();
    for field in wire::fields(payload) {
        let field = field?;
        match field.number {
            1 => matrix.a = field.f32()?,
            2 => matrix.b = field.f32()?,
            3 => matrix.c = field.f32()?,
            4 => matrix.d = field.f32()?,
            5 => matrix.tx = field.f32()?,
            6 => matrix.ty = field.f32()?,
            _ => {}
        }
    }
    Ok(matrix)
}

fn audio(payload: &[u8], budget: &Budget) -> Result<Audio> {
    budget.spend()?;
    fold(payload, Audio::default(), |audio, field| {
        Ok(match field.number {
            1 => Audio {
                audio_key: field.str()?.to_owned(),
                ..audio
            },
            2 => Audio {
                start_frame: field.i32()?,
                ..audio
            },
            3 => Audio {
                end_frame: field.i32()?,
                ..audio
            },
            4 => Audio {
                start_time: field.i32()?,
                ..audio
            },
            5 => Audio {
                total_time: field.i32()?,
                ..audio
            },
            _ => audio,
        })
    })
}
