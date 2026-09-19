//! Which images a movie actually draws, and how large.
//!
//! These are plain numbers for tools such as optimizers; the crate itself
//! draws nothing and makes no decision from them.
use super::{Frame, ImageInfo, Movie, image_name};
use std::collections::{BTreeSet, HashMap};

/// Extent of a sprite on the canvas, in view-box units: the layout size
/// stretched by the frame transform. Rotation does not inflate it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DrawnSize {
    pub width: f32,
    pub height: f32,
}

impl DrawnSize {
    fn of(frame: &Frame) -> Option<Self> {
        let (scale_x, scale_y) = frame.matrix().scale();
        let size = Self {
            width: (frame.layout.width * scale_x).abs(),
            height: (frame.layout.height * scale_y).abs(),
        };
        (size.width.is_finite() && size.height.is_finite()).then_some(size)
    }

    /// The larger of each dimension, taken independently.
    fn max(self, other: Self) -> Self {
        Self {
            width: self.width.max(other.width),
            height: self.height.max(other.height),
        }
    }
}

/// How one `images` entry is used across the whole movie.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageUsage {
    pub key: String,
    /// Sprites drawing this image, matte sprites included.
    pub sprites: usize,
    /// Frames of those sprites with `alpha > 0`.
    pub visible_frames: usize,
    /// Largest extent over all visible frames; `None` if it is never drawn.
    pub max_drawn_size: Option<DrawnSize>,
}

impl Movie {
    /// Every key named by a sprite (`imageKey`, `matteKey`, both without a
    /// `.matte` suffix) or by an audio entity.
    pub fn referenced_keys(&self) -> BTreeSet<&str> {
        let sprites = self.sprites.iter().flat_map(|sprite| {
            [sprite.image_name(), image_name(&sprite.matte_key)]
                .into_iter()
                .filter(|key| !key.is_empty())
        });
        let audios = self.audios.iter().map(|audio| audio.audio_key.as_str());
        sprites.chain(audios).collect()
    }

    /// Stored images that nothing references.
    pub fn unused_image_keys(&self) -> Vec<&str> {
        let referenced = self.referenced_keys();
        self.images
            .iter()
            .map(|image| image.key.as_str())
            .filter(|key| !referenced.contains(key))
            .collect()
    }

    /// Referenced keys with no stored image. Sprites that only draw vector
    /// shapes usually name a key without an image, so this is not an error list.
    pub fn missing_image_keys(&self) -> Vec<&str> {
        let stored: BTreeSet<&str> = self.images.iter().map(|image| image.key.as_str()).collect();
        self.referenced_keys()
            .into_iter()
            .filter(|key| !stored.contains(key))
            .collect()
    }

    /// Usage of each stored image, in `images` order.
    pub fn image_usage(&self) -> Vec<ImageUsage> {
        // One pass over the sprites, however many images there are.
        let mut tallies: HashMap<&str, Tally> = HashMap::new();
        for sprite in &self.sprites {
            let tally = tallies.entry(sprite.image_name()).or_default();
            *tally = tally.with(&sprite.frames);
        }
        let usage = |image: &ImageInfo| {
            let tally = tallies.get(image.key.as_str()).copied().unwrap_or_default();
            ImageUsage {
                key: image.key.clone(),
                sprites: tally.sprites,
                visible_frames: tally.visible_frames,
                max_drawn_size: tally.max_drawn_size,
            }
        };
        self.images.iter().map(usage).collect()
    }
}

/// Running totals for one image key.
#[derive(Clone, Copy, Default)]
struct Tally {
    sprites: usize,
    visible_frames: usize,
    max_drawn_size: Option<DrawnSize>,
}

impl Tally {
    /// The totals after one more sprite with these frames.
    fn with(self, frames: &[Frame]) -> Self {
        let visible = || frames.iter().filter(|frame| frame.is_visible());
        let largest = visible()
            .filter_map(DrawnSize::of)
            .chain(self.max_drawn_size)
            .reduce(DrawnSize::max);
        Self {
            sprites: self.sprites + 1,
            visible_frames: self.visible_frames + visible().count(),
            max_drawn_size: largest,
        }
    }
}
