//! Edits of the `images` map. Each returns a new document; every part that is
//! not named by the edit is carried over with its bytes untouched.
use super::{
    Document, IMAGES,
    part::{self, Part},
};
use crate::{
    ValueKind,
    error::{Error, Result},
};

const KEY_NOT_FOUND: Error = Error::invalid_edit("svga_image_key_not_found");
const KEY_AMBIGUOUS: Error = Error::invalid_edit("svga_image_key_ambiguous");
const KEY_EXISTS: Error = Error::invalid_edit("svga_image_key_exists");
const INDEX_OUT_OF_RANGE: Error = Error::invalid_edit("svga_image_index_out_of_range");

impl Document {
    /// Index of the single part storing `key`. A repeated key is refused
    /// rather than guessed at.
    fn position(&self, key: &str) -> Result<usize> {
        let mut matches = self.parts.iter().enumerate().filter(|(_, part)| {
            part.image()
                .is_some_and(|image| image.key_bytes() == key.as_bytes())
        });
        let (index, _) = matches.next().ok_or(KEY_NOT_FOUND)?;
        if matches.next().is_some() {
            return Err(KEY_AMBIGUOUS);
        }
        Ok(index)
    }

    /// Index of the part holding the `entry`-th item of [`Document::images`].
    fn entry_position(&self, entry: usize) -> Result<usize> {
        let mut images = self.parts.iter().enumerate();
        images
            .by_ref()
            .filter(|(_, part)| part.image().is_some())
            .nth(entry)
            .map(|(index, _)| index)
            .ok_or(INDEX_OUT_OF_RANGE)
    }

    fn with_parts(&self, parts: Vec<Part>) -> Self {
        Self { parts }
    }

    fn with_value_at(&self, index: usize, value: &[u8]) -> Result<Self> {
        let mut parts = self.parts.clone();
        if let Some(slot) = parts.get_mut(index) {
            let entry = part::entry_with_value(slot.field().payload, value)?;
            *slot = Part::image_entry(&entry)?;
        }
        Ok(self.with_parts(parts))
    }

    fn without_part(&self, index: usize) -> Self {
        let kept = self.parts.iter().enumerate();
        let kept = kept.filter(|(position, _)| *position != index);
        self.with_parts(kept.map(|(_, part)| part.clone()).collect())
    }

    /// Replace the value stored under `key`. The key field, any unknown entry
    /// fields and the entry's position are kept as stored.
    #[must_use = "edits return a new document"]
    pub fn replace_image(&self, key: &str, value: &[u8]) -> Result<Self> {
        self.with_value_at(self.position(key)?, value)
    }

    /// Remove the entry stored under `key`.
    #[must_use = "edits return a new document"]
    pub fn remove_image(&self, key: &str) -> Result<Self> {
        Ok(self.without_part(self.position(key)?))
    }

    /// [`Document::replace_image`] addressed by position in
    /// [`Document::images`] instead of by key. This reaches entries a key
    /// cannot name: repeated keys, and keys that are not valid UTF-8.
    #[must_use = "edits return a new document"]
    pub fn replace_image_at(&self, entry: usize, value: &[u8]) -> Result<Self> {
        self.with_value_at(self.entry_position(entry)?, value)
    }

    /// [`Document::remove_image`] addressed by position in
    /// [`Document::images`]. Later entries move up by one.
    #[must_use = "edits return a new document"]
    pub fn remove_image_at(&self, entry: usize) -> Result<Self> {
        Ok(self.without_part(self.entry_position(entry)?))
    }

    /// Every entry whose value is a file name that `resolve` knows, with the
    /// file's bytes embedded instead. One pass, by position, so repeated keys
    /// are fine; refused once the payload would pass `max_bytes`.
    pub(crate) fn with_embedded<'a>(
        &self,
        resolve: impl Fn(&str) -> Option<&'a [u8]>,
        max_bytes: usize,
    ) -> Result<Self> {
        let mut total = 0usize;
        let mut parts = Vec::with_capacity(self.parts.len());
        for part in &self.parts {
            let external = part
                .image()
                .filter(|image| image.kind() == ValueKind::FileName)
                .and_then(|image| resolve(std::str::from_utf8(image.value()).ok()?));
            let part = match external {
                Some(bytes) => {
                    Part::image_entry(&part::entry_with_value(part.field().payload, bytes)?)?
                }
                None => part.clone(),
            };
            total = total
                .checked_add(part.raw().len())
                .filter(|total| *total <= max_bytes)
                .ok_or(Error::limit("svga_inflated_size_exceeds_limit"))?;
            parts.push(part);
        }
        Ok(self.with_parts(parts))
    }

    /// Add an entry for a key that is not present yet. It goes after the last
    /// existing entry, or where the `images` field belongs when there is none.
    #[must_use = "edits return a new document"]
    pub fn insert_image(&self, key: &str, value: &[u8]) -> Result<Self> {
        if self.position(key) != Err(KEY_NOT_FOUND) {
            return Err(KEY_EXISTS);
        }
        let last_image = self.parts.iter().rposition(|part| part.number == IMAGES);
        let index = match last_image {
            Some(index) => index + 1,
            None => self
                .parts
                .iter()
                .position(|part| part.number > IMAGES)
                .unwrap_or(self.parts.len()),
        };
        let entry = Part::image_entry(&part::entry_payload(key.as_bytes(), value))?;
        let (before, after) = self.parts.split_at(index);
        let parts = [before, &[entry], after].concat();
        Ok(self.with_parts(parts))
    }
}
