//! SVGA 1.x `movie.spec` JSON → [`Movie`]. Absent or mistyped members fall
//! back to their defaults, the way the protobuf decoder treats absent fields.
use super::{ArchiveFile, find_file, spec_shape};
use crate::{
    Limits, Movie, ValueKind,
    error::{Error, Result},
    movie::{Frame, ImageInfo, Layout, Params, Sprite, Transform},
};
use serde_json::Value;
use std::cell::Cell;

/// Counts decoded elements, like the protobuf decoder does.
pub(super) struct Budget(Cell<usize>);

impl Budget {
    pub fn spend(&self) -> Result<()> {
        let left = self.0.get().checked_sub(1);
        self.0
            .set(left.ok_or(Error::limit("svga_too_many_elements"))?);
        Ok(())
    }
}

pub(super) fn number(value: &Value, member: &str) -> f32 {
    value.get(member).and_then(Value::as_f64).unwrap_or(0.0) as f32
}

fn integer(value: &Value, member: &str) -> i32 {
    value.get(member).and_then(Value::as_f64).unwrap_or(0.0) as i32
}

pub(super) fn text(value: &Value, member: &str) -> String {
    let text = value.get(member).and_then(Value::as_str);
    text.unwrap_or_default().to_owned()
}

pub(super) fn items<'a>(value: &'a Value, member: &str) -> &'a [Value] {
    let items = value.get(member).and_then(Value::as_array);
    items.map(Vec::as_slice).unwrap_or_default()
}

/// The movie, and for each image key the container file name it maps to.
pub(super) fn movie(
    spec: &[u8],
    files: &[ArchiveFile],
    limits: &Limits,
) -> Result<(Movie, Vec<(String, String)>)> {
    let root: Value =
        serde_json::from_slice(spec).map_err(|_| Error::malformed("svga_invalid_spec"))?;
    if !root.is_object() {
        return Err(Error::malformed("svga_invalid_spec"));
    }
    let budget = Budget(Cell::new(limits.max_elements));
    let header = root.get("movie").unwrap_or(&Value::Null);
    let view_box = header.get("viewBox").unwrap_or(&Value::Null);
    let names = names(&root);
    let movie = Movie {
        version: text(&root, "ver"),
        params: Params {
            view_box_width: number(view_box, "width"),
            view_box_height: number(view_box, "height"),
            fps: integer(header, "fps"),
            frames: integer(header, "frames"),
        },
        images: images(&names, files),
        sprites: items(&root, "sprites")
            .iter()
            .map(|value| sprite(value, &budget))
            .collect::<Result<_>>()?,
        audios: Vec::new(),
    };
    Ok((movie, names))
}

/// Image key → file name. A name that is not a string falls back to the key.
fn names(root: &Value) -> Vec<(String, String)> {
    let entries = root.get("images").and_then(Value::as_object);
    let pair = |(key, name): (&String, &Value)| {
        let name = name.as_str().unwrap_or(key);
        (key.clone(), name.to_owned())
    };
    entries.into_iter().flatten().map(pair).collect()
}

/// Each image described by the container file it names, when that exists.
fn images(names: &[(String, String)], files: &[ArchiveFile]) -> Vec<ImageInfo> {
    let describe = |(key, name): &(String, String)| {
        let stored = find_file(files, name);
        ImageInfo {
            key: key.clone(),
            byte_len: stored.map_or(name.len(), <[u8]>::len),
            kind: stored.map_or(ValueKind::FileName, ValueKind::sniff),
        }
    };
    names.iter().map(describe).collect()
}

fn sprite(value: &Value, budget: &Budget) -> Result<Sprite> {
    budget.spend()?;
    Ok(Sprite {
        image_key: text(value, "imageKey"),
        matte_key: text(value, "matteKey"),
        frames: items(value, "frames")
            .iter()
            .map(|value| frame(value, budget))
            .collect::<Result<_>>()?,
    })
}

fn frame(value: &Value, budget: &Budget) -> Result<Frame> {
    budget.spend()?;
    let layout = value.get("layout").unwrap_or(&Value::Null);
    Ok(Frame {
        alpha: number(value, "alpha"),
        layout: Layout {
            x: number(layout, "x"),
            y: number(layout, "y"),
            width: number(layout, "width"),
            height: number(layout, "height"),
        },
        transform: transform(value),
        clip_path: text(value, "clipPath"),
        shapes: items(value, "shapes")
            .iter()
            .map(|value| spec_shape::shape(value, budget))
            .collect::<Result<_>>()?,
    })
}

pub(super) fn transform(owner: &Value) -> Option<Transform> {
    let matrix = owner.get("transform").filter(|value| value.is_object())?;
    Some(Transform {
        a: number(matrix, "a"),
        b: number(matrix, "b"),
        c: number(matrix, "c"),
        d: number(matrix, "d"),
        tx: number(matrix, "tx"),
        ty: number(matrix, "ty"),
    })
}
