use crate::{Operation, apply};
use anyhow::Result;
use image::RgbaImage;
use std::{collections::VecDeque, sync::Arc};

// Additional rendered states, excluding the original and current image.
const CACHE_BYTES: usize = 64 * 1024 * 1024;
const CACHE_ENTRIES: usize = 16;

struct RenderCache {
    entries: VecDeque<(usize, Arc<RgbaImage>)>,
    bytes: usize,
    limit: usize,
}

impl RenderCache {
    fn new(limit: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
            limit,
        }
    }

    fn retain(&mut self, keep: impl Fn(usize) -> bool) {
        self.entries.retain(|(cursor, _)| keep(*cursor));
        self.bytes = self
            .entries
            .iter()
            .map(|(_, image)| image.as_raw().len())
            .sum();
    }

    fn remember(&mut self, cursor: usize, image: &Arc<RgbaImage>) {
        let bytes = image.as_raw().len();
        if cursor == 0 || bytes > self.limit {
            return;
        }
        self.retain(|index| index != cursor);
        while self.bytes + bytes > self.limit || self.entries.len() >= CACHE_ENTRIES {
            if let Some((_, image)) = self.entries.pop_front() {
                self.bytes -= image.as_raw().len();
            }
        }
        self.entries.push_back((cursor, image.clone()));
        self.bytes += bytes;
    }
}

/// Transactional edit history with incremental edits and bounded undo checkpoints.
pub struct History {
    original: Arc<RgbaImage>,
    image: Arc<RgbaImage>,
    entries: Vec<(u64, Operation)>,
    cursor: usize,
    next_id: u64,
    cache: RenderCache,
}

impl History {
    pub fn new(image: RgbaImage) -> Self {
        let original = Arc::new(image);
        Self {
            image: original.clone(),
            original,
            entries: Vec::new(),
            cursor: 0,
            next_id: 1,
            cache: RenderCache::new(CACHE_BYTES),
        }
    }
    pub fn image(&self) -> &Arc<RgbaImage> {
        &self.image
    }
    pub fn state_id(&self) -> u64 {
        self.cursor.checked_sub(1).map_or(0, |i| self.entries[i].0)
    }
    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }
    pub fn can_redo(&self) -> bool {
        self.cursor < self.entries.len()
    }

    pub fn apply(&mut self, op: Operation) -> Result<()> {
        // Compute before changing history, so failures preserve pixels and redo.
        let image = Arc::new(apply(&self.image, &op)?);
        self.entries.truncate(self.cursor);
        self.cache.retain(|cursor| cursor <= self.cursor);
        self.cache.remember(self.cursor, &self.image);
        self.entries.push((self.next_id, op));
        self.next_id += 1;
        self.cursor += 1;
        self.image = image;
        Ok(())
    }

    pub fn undo(&mut self) -> Result<()> {
        self.move_to(self.cursor.saturating_sub(1))
    }
    pub fn redo(&mut self) -> Result<()> {
        self.move_to((self.cursor + 1).min(self.entries.len()))
    }

    fn move_to(&mut self, cursor: usize) -> Result<()> {
        if cursor == self.cursor {
            return Ok(());
        }
        let (mut start, mut image) = self
            .cache
            .entries
            .iter()
            .filter(|(index, _)| *index <= cursor)
            .max_by_key(|(index, _)| *index)
            .map(|(index, image)| (*index, image.clone()))
            .unwrap_or((0, self.original.clone()));
        if self.cursor <= cursor && self.cursor > start {
            start = self.cursor;
            image = self.image.clone();
        }
        for (_, op) in &self.entries[start..cursor] {
            image = Arc::new(apply(&image, op)?);
        }
        self.cache.remember(self.cursor, &self.image);
        self.cursor = cursor;
        self.image = image;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PixelRect, Rotation};
    use image::Rgba;

    #[test]
    fn cached_and_evicted_history_matches_sequential_pixels() {
        let input = RgbaImage::from_fn(12, 8, |x, y| {
            Rgba([(x * 20) as u8, (y * 30) as u8, 70, 128])
        });
        let ops = vec![
            Operation::Resize {
                width: 8,
                height: 6,
            },
            Operation::Mosaic {
                regions: vec![PixelRect::new(0, 0, 4, 4)],
                block: 2,
            },
            Operation::Rotate(Rotation::Clockwise),
            Operation::Highlight {
                points: vec![[1.0, 1.0], [4.0, 4.0]],
                width: 2.0,
                color: [255, 0, 0, 128],
            },
            Operation::Crop(PixelRect::new(1, 1, 3, 3)),
            Operation::Flatten([255; 3]),
        ];
        for limit in [0, 200, CACHE_BYTES] {
            let mut history = History::new(input.clone());
            history.cache = RenderCache::new(limit);
            let mut states = vec![input.clone()];
            let mut ids = vec![0];
            for op in &ops {
                states.push(apply(states.last().unwrap(), op).unwrap());
                history.apply(op.clone()).unwrap();
                ids.push(history.state_id());
                assert_eq!(history.image.as_ref(), states.last().unwrap());
                assert!(history.cache.bytes <= limit);
            }
            for i in (0..ops.len()).rev() {
                history.undo().unwrap();
                assert_eq!(*history.image, states[i]);
                assert_eq!(history.state_id(), ids[i]);
            }
            for i in 1..=ops.len() {
                history.redo().unwrap();
                assert_eq!(*history.image, states[i]);
                assert_eq!(history.state_id(), ids[i]);
            }
            history.undo().unwrap();
            let before = history.image.clone();
            assert!(
                history
                    .apply(Operation::Resize {
                        width: 0,
                        height: 1
                    })
                    .is_err()
            );
            assert!(Arc::ptr_eq(&history.image, &before));
            assert!(history.can_redo());
            history.apply(Operation::Flatten([0; 3])).unwrap();
            assert_eq!(
                *history.image,
                apply(&before, &Operation::Flatten([0; 3])).unwrap()
            );
            assert!(!history.can_redo());
            assert!(!ids.contains(&history.state_id()));
            let branch = history.image.clone();
            history.undo().unwrap();
            history.redo().unwrap();
            assert_eq!(*history.image, *branch);
        }
    }

    #[test]
    fn checkpoint_cache_limits_bytes_and_entries() {
        let image = Arc::new(RgbaImage::new(4, 4));
        let mut cache = RenderCache::new(128);
        for cursor in 1..=20 {
            cache.remember(cursor, &image);
        }
        assert_eq!(cache.bytes, 128);
        assert_eq!(cache.entries.len(), 2);
        cache.remember(20, &image);
        assert_eq!(cache.bytes, 128);
        cache.remember(21, &Arc::new(RgbaImage::new(10, 10)));
        assert_eq!(cache.bytes, 128);
        cache.retain(|cursor| cursor <= 19);
        assert_eq!(cache.bytes, 64);
        let mut cache = RenderCache::new(CACHE_BYTES);
        for cursor in 1..=100 {
            cache.remember(cursor, &image);
        }
        assert_eq!(cache.entries.len(), CACHE_ENTRIES);
    }
}
