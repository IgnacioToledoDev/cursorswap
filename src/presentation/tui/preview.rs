//! Preview data for the TUI: pixel conversion, animation timing and caching.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use image::imageops::FilterType;
use image::{DynamicImage, RgbaImage};
use ratatui::layout::Size;
use ratatui_image::Resize;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::Protocol;

use crate::domain::cursor::CursorTheme;
use crate::domain::ports::{Preview, PreviewFrame, PreviewLoader};

/// Frame delays below this are raised to it so a zero delay cannot busy-loop.
pub const MIN_FRAME_DELAY_MS: u32 = 30;

/// Converts premultiplied RGBA (as stored in Xcursor files) to straight alpha.
pub fn unpremultiply(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for pixel in rgba.chunks_exact(4) {
        let alpha = u32::from(pixel[3]);
        if alpha == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        for &channel in &pixel[..3] {
            // Invalid input (channel > alpha) saturates instead of wrapping.
            let straight = (u32::from(channel) * 255 / alpha).min(255);
            out.push(straight as u8);
        }
        out.push(pixel[3]);
    }
    out
}

/// Builds a straight-alpha image from a preview frame; `None` if the pixel
/// buffer does not match the declared dimensions.
pub fn frame_to_image(frame: &PreviewFrame) -> Option<DynamicImage> {
    let straight = unpremultiply(&frame.rgba);
    RgbaImage::from_raw(frame.width, frame.height, straight).map(DynamicImage::ImageRgba8)
}

/// Tracks which animation frame is current as time elapses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationClock {
    delays_ms: Vec<u32>,
    position_ms: u128,
}

impl AnimationClock {
    pub fn new(delays_ms: &[u32]) -> Self {
        AnimationClock {
            delays_ms: delays_ms
                .iter()
                .map(|d| (*d).max(MIN_FRAME_DELAY_MS))
                .collect(),
            position_ms: 0,
        }
    }

    pub fn is_animated(&self) -> bool {
        self.delays_ms.len() > 1
    }

    pub fn index(&self) -> usize {
        let mut remaining = self.position_ms;
        for (index, delay) in self.delays_ms.iter().enumerate() {
            if remaining < u128::from(*delay) {
                return index;
            }
            remaining -= u128::from(*delay);
        }
        0
    }

    /// Advances by `elapsed`; returns whether the current frame changed.
    pub fn advance(&mut self, elapsed: Duration) -> bool {
        if !self.is_animated() {
            return false;
        }
        let before = self.index();
        let cycle: u128 = self.delays_ms.iter().map(|d| u128::from(*d)).sum();
        self.position_ms = (self.position_ms + elapsed.as_millis()) % cycle;
        self.index() != before
    }
}

pub struct LoadedPreview {
    images: Vec<DynamicImage>,
    clock: AnimationClock,
    protocols: HashMap<usize, Protocol>,
    protocol_size: Option<Size>,
}

impl LoadedPreview {
    pub fn from_preview(preview: &Preview) -> Result<Self, String> {
        if preview.frames.is_empty() {
            return Err("cursor has no frames".to_string());
        }
        let images = preview
            .frames
            .iter()
            .map(|frame| frame_to_image(frame).ok_or_else(|| "corrupt cursor frame".to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        let delays: Vec<u32> = preview.frames.iter().map(|f| f.delay_ms).collect();
        Ok(LoadedPreview {
            images,
            clock: AnimationClock::new(&delays),
            protocols: HashMap::new(),
            protocol_size: None,
        })
    }

    #[cfg(test)]
    pub fn frame_count(&self) -> usize {
        self.images.len()
    }

    #[cfg(test)]
    pub fn frame_index(&self) -> usize {
        self.clock.index()
    }

    pub fn tick(&mut self, elapsed: Duration) -> bool {
        self.clock.advance(elapsed)
    }

    /// Encodes the current frame for `size` cells unless already cached.
    pub fn prepare(&mut self, picker: &Picker, size: Size) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        if self.protocol_size != Some(size) {
            self.protocols.clear();
            self.protocol_size = Some(size);
        }
        let index = self.clock.index();
        if self.protocols.contains_key(&index) {
            return;
        }
        // Nearest-neighbor keeps small cursor sprites crisp when scaled up.
        let resize = Resize::Scale(Some(FilterType::Nearest));
        if let Ok(protocol) = picker.new_protocol(self.images[index].clone(), size, resize) {
            self.protocols.insert(index, protocol);
        }
    }

    pub fn current_protocol(&self) -> Option<&Protocol> {
        self.protocols.get(&self.clock.index())
    }
}

pub enum PreviewEntry {
    Loaded(LoadedPreview),
    Failed(String),
}

/// Loads each theme's preview at most once, caching failures too.
#[derive(Default)]
pub struct PreviewCache {
    entries: HashMap<PathBuf, PreviewEntry>,
}

impl PreviewCache {
    pub fn ensure(&mut self, theme: &CursorTheme, loader: &dyn PreviewLoader) {
        if self.entries.contains_key(&theme.path) {
            return;
        }
        let entry = match loader.load(theme) {
            Ok(preview) => match LoadedPreview::from_preview(&preview) {
                Ok(loaded) => PreviewEntry::Loaded(loaded),
                Err(message) => PreviewEntry::Failed(message),
            },
            Err(err) => PreviewEntry::Failed(err.to_string()),
        };
        self.entries.insert(theme.path.clone(), entry);
    }

    pub fn get(&self, path: &Path) -> Option<&PreviewEntry> {
        self.entries.get(path)
    }

    pub fn get_mut(&mut self, path: &Path) -> Option<&mut PreviewEntry> {
        self.entries.get_mut(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::{FakePreviewLoader, theme};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn frame(side: u32, delay_ms: u32, pixel: [u8; 4]) -> PreviewFrame {
        PreviewFrame {
            width: side,
            height: side,
            rgba: pixel.repeat((side * side) as usize),
            delay_ms,
        }
    }

    // --- unpremultiply ---

    #[test]
    fn opaque_pixels_are_unchanged() {
        assert_eq!(unpremultiply(&[10, 200, 255, 255]), [10, 200, 255, 255]);
    }

    #[test]
    fn half_alpha_pixels_are_scaled_back_up() {
        // 64 * 255 / 128 = 127 (integer division)
        assert_eq!(unpremultiply(&[64, 32, 0, 128]), [127, 63, 0, 128]);
    }

    #[test]
    fn fully_transparent_pixels_stay_transparent() {
        assert_eq!(unpremultiply(&[9, 9, 9, 0]), [0, 0, 0, 0]);
    }

    #[test]
    fn invalid_premultiplied_values_saturate_without_panicking() {
        assert_eq!(unpremultiply(&[255, 200, 100, 10]), [255, 255, 255, 10]);
        assert_eq!(unpremultiply(&[255, 255, 255, 1]), [255, 255, 255, 1]);
    }

    #[test]
    fn converts_every_pixel_of_a_buffer() {
        let out = unpremultiply(&[1, 2, 3, 255, 0, 0, 0, 0]);
        assert_eq!(out, [1, 2, 3, 255, 0, 0, 0, 0]);
    }

    #[test]
    fn a_trailing_partial_pixel_is_dropped() {
        assert_eq!(unpremultiply(&[1, 2, 3, 255, 7, 7]).len(), 4);
    }

    // --- frame_to_image ---

    #[test]
    fn frame_becomes_a_straight_alpha_image() {
        let image = frame_to_image(&frame(2, 0, [64, 0, 0, 128])).unwrap();
        assert_eq!((image.width(), image.height()), (2, 2));
        assert_eq!(&image.to_rgba8().as_raw()[..4], [127, 0, 0, 128]);
    }

    #[test]
    fn mismatched_buffer_is_rejected() {
        let mut bad = frame(2, 0, [0; 4]);
        bad.rgba.pop();
        assert!(frame_to_image(&bad).is_none());
    }

    // --- animation clock ---

    #[test]
    fn a_single_frame_never_animates() {
        let mut clock = AnimationClock::new(&[100]);
        assert!(!clock.is_animated());
        assert!(!clock.advance(Duration::from_secs(10)));
        assert_eq!(clock.index(), 0);
    }

    #[test]
    fn no_frames_behaves_like_a_static_image() {
        let mut clock = AnimationClock::new(&[]);
        assert!(!clock.advance(ms(500)));
        assert_eq!(clock.index(), 0);
    }

    #[test]
    fn elapsed_time_accumulates_across_calls() {
        let mut clock = AnimationClock::new(&[100, 100, 100]);
        assert!(!clock.advance(ms(50)));
        assert_eq!(clock.index(), 0);
        assert!(clock.advance(ms(60)));
        assert_eq!(clock.index(), 1);
        assert!(clock.advance(ms(90)));
        assert_eq!(clock.index(), 2);
    }

    #[test]
    fn one_large_step_can_skip_several_frames() {
        let mut clock = AnimationClock::new(&[100, 100, 100, 100]);
        clock.advance(ms(250));
        assert_eq!(clock.index(), 2);
    }

    #[test]
    fn zero_delay_is_clamped_to_the_minimum() {
        let mut clock = AnimationClock::new(&[0, 0]);
        assert!(!clock.advance(ms(29)));
        assert_eq!(clock.index(), 0);
        assert!(clock.advance(ms(1)));
        assert_eq!(clock.index(), 1);
    }

    #[test]
    fn short_delays_are_raised_to_the_minimum() {
        let mut clock = AnimationClock::new(&[5, 5]);
        clock.advance(ms(29));
        assert_eq!(clock.index(), 0);
    }

    #[test]
    fn the_animation_wraps_around() {
        let mut clock = AnimationClock::new(&[100, 100]);
        clock.advance(ms(250));
        assert_eq!(clock.index(), 0);
        clock.advance(ms(60));
        assert_eq!(clock.index(), 1);
    }

    #[test]
    fn a_huge_elapsed_time_lands_on_the_right_frame_without_looping() {
        let mut clock = AnimationClock::new(&[30, 40]);
        let elapsed_ms: u64 = u64::from(u32::MAX) * 1000 + 35;
        clock.advance(ms(elapsed_ms));
        let into_cycle = elapsed_ms % 70;
        assert_eq!(clock.index(), usize::from(into_cycle >= 30));
    }

    #[test]
    fn a_zero_elapsed_step_changes_nothing() {
        let mut clock = AnimationClock::new(&[50, 50]);
        assert!(!clock.advance(Duration::ZERO));
        assert_eq!(clock.index(), 0);
    }

    // --- loaded preview ---

    fn preview(frames: Vec<PreviewFrame>) -> Preview {
        Preview { frames }
    }

    #[test]
    fn loaded_preview_exposes_frames_and_ticks() {
        let mut loaded = LoadedPreview::from_preview(&preview(vec![
            frame(2, 100, [255; 4]),
            frame(2, 100, [255; 4]),
        ]))
        .unwrap();
        assert_eq!(loaded.frame_count(), 2);
        assert!(loaded.tick(ms(120)));
        assert_eq!(loaded.frame_index(), 1);
    }

    #[test]
    fn a_preview_without_frames_is_an_error() {
        assert!(LoadedPreview::from_preview(&preview(vec![])).is_err());
    }

    #[test]
    fn a_preview_with_a_corrupt_frame_is_an_error() {
        let mut bad = frame(2, 0, [0; 4]);
        bad.rgba.pop();
        assert!(LoadedPreview::from_preview(&preview(vec![frame(2, 0, [0; 4]), bad])).is_err());
    }

    #[test]
    fn protocol_is_encoded_lazily_for_the_requested_size() {
        let mut loaded =
            LoadedPreview::from_preview(&preview(vec![frame(4, 0, [255, 0, 0, 255])])).unwrap();
        assert!(loaded.current_protocol().is_none());
        loaded.prepare(&Picker::halfblocks(), Size::new(10, 5));
        let size = loaded.current_protocol().unwrap().size();
        assert!(size.width <= 10 && size.height <= 5 && size.width > 0);
    }

    #[test]
    fn a_new_size_replaces_the_cached_protocols() {
        let mut loaded =
            LoadedPreview::from_preview(&preview(vec![frame(4, 0, [255, 0, 0, 255])])).unwrap();
        let picker = Picker::halfblocks();
        loaded.prepare(&picker, Size::new(10, 5));
        let small = loaded.current_protocol().unwrap().size();
        loaded.prepare(&picker, Size::new(30, 15));
        assert!(loaded.current_protocol().unwrap().size().width > small.width);
    }

    #[test]
    fn a_zero_size_prepares_nothing() {
        let mut loaded =
            LoadedPreview::from_preview(&preview(vec![frame(4, 0, [255; 4])])).unwrap();
        loaded.prepare(&Picker::halfblocks(), Size::new(0, 0));
        assert!(loaded.current_protocol().is_none());
    }

    // --- cache ---

    #[test]
    fn each_theme_is_loaded_once() {
        let loader = FakePreviewLoader::returning(preview(vec![frame(2, 0, [255; 4])]));
        let mut cache = PreviewCache::default();
        let a = theme("a", None);
        cache.ensure(&a, &loader);
        cache.ensure(&a, &loader);
        assert_eq!(loader.loads.borrow().len(), 1);
        assert!(matches!(cache.get(&a.path), Some(PreviewEntry::Loaded(_))));
    }

    #[test]
    fn failures_are_cached_too() {
        let loader = FakePreviewLoader::failing("broken");
        let mut cache = PreviewCache::default();
        let a = theme("a", None);
        cache.ensure(&a, &loader);
        cache.ensure(&a, &loader);
        assert_eq!(loader.loads.borrow().len(), 1);
        assert!(
            matches!(cache.get(&a.path), Some(PreviewEntry::Failed(m)) if m.contains("broken"))
        );
    }

    #[test]
    fn an_undecodable_preview_is_cached_as_failed() {
        let loader = FakePreviewLoader::returning(preview(vec![]));
        let mut cache = PreviewCache::default();
        let a = theme("a", None);
        cache.ensure(&a, &loader);
        assert!(matches!(cache.get(&a.path), Some(PreviewEntry::Failed(_))));
    }

    #[test]
    fn other_themes_load_independently() {
        let loader = FakePreviewLoader::returning(preview(vec![frame(2, 0, [255; 4])]));
        let mut cache = PreviewCache::default();
        let (a, b) = (theme("a", None), theme("b", None));
        cache.ensure(&a, &loader);
        cache.ensure(&b, &loader);
        assert_eq!(*loader.loads.borrow(), [a.path.clone(), b.path.clone()]);
        assert!(cache.get_mut(&b.path).is_some());
        assert!(cache.get(Path::new("/other")).is_none());
    }
}
