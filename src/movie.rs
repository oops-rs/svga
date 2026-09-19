//! The typed view: a read-only picture of a movie, decoded on demand.
//!
//! This is for reading. It drops anything outside the schema, so edits belong
//! on the [`Document`](crate::Document), which keeps every byte.
pub(crate) mod decode;
mod decode_shape;
pub(crate) mod encode;
mod shape;
mod usage;

#[cfg(test)]
mod tests;

pub use crate::document::ValueKind;
pub use shape::{Geometry, LineCap, LineJoin, Rgba, Shape, ShapeStyle};
pub use usage::{DrawnSize, ImageUsage};

const MATTE_SUFFIX: &str = ".matte";

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Movie {
    /// As stored, for example `2.1.0` or `1.1.0`.
    pub version: String,
    pub params: Params,
    /// One entry per distinct key, in first-seen order. The bytes stay in the
    /// document or archive.
    pub images: Vec<ImageInfo>,
    pub sprites: Vec<Sprite>,
    pub audios: Vec<Audio>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Params {
    pub view_box_width: f32,
    pub view_box_height: f32,
    pub fps: i32,
    pub frames: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageInfo {
    pub key: String,
    /// Size of the stored value: the image itself, or just a file name.
    pub byte_len: usize,
    pub kind: ValueKind,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sprite {
    pub image_key: String,
    pub frames: Vec<Frame>,
    /// `image_key` of the sprite that masks this one; empty when unmasked.
    pub matte_key: String,
}

impl Sprite {
    /// The `images` key this sprite draws. A matte sprite is named
    /// `<key>.matte` and draws `<key>`.
    pub fn image_name(&self) -> &str {
        image_name(&self.image_key)
    }

    pub fn is_matte(&self) -> bool {
        self.image_key.ends_with(MATTE_SUFFIX)
    }
}

pub(crate) fn image_name(key: &str) -> &str {
    key.strip_suffix(MATTE_SUFFIX).unwrap_or(key)
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Frame {
    pub alpha: f32,
    pub layout: Layout,
    /// `None` when the frame stores no transform; see [`Frame::matrix`].
    pub transform: Option<Transform>,
    pub clip_path: String,
    pub shapes: Vec<Shape>,
}

impl Frame {
    /// The stored transform, or the identity players assume without one.
    pub fn matrix(&self) -> Transform {
        self.transform.unwrap_or(Transform::IDENTITY)
    }

    /// Players skip frames that are fully transparent.
    pub fn is_visible(&self) -> bool {
        self.alpha > 0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Layout {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// A 2D affine matrix: `x' = a·x + c·y + tx`, `y' = b·x + d·y + ty`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Transform {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Transform {
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    /// How much the matrix stretches the x and y axes.
    pub fn scale(&self) -> (f32, f32) {
        (self.a.hypot(self.b), self.c.hypot(self.d))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Audio {
    /// Key of the MP3 in the `images` map.
    pub audio_key: String,
    pub start_frame: i32,
    pub end_frame: i32,
    pub start_time: i32,
    pub total_time: i32,
}
