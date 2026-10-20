use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use xcursor::parser::{Image, parse_xcursor};

use crate::domain::cursor::CursorTheme;
use crate::domain::error::PreviewError;
use crate::domain::ports::{CursorState, Preview, PreviewFrame, PreviewLoader};

const TARGET_SIZE: u32 = 32;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_DIMENSION: u32 = 256;
const MAX_FRAMES: usize = 64;

const HEADER_LEN: usize = 16;
const TOC_ENTRY_LEN: usize = 12;
const IMAGE_CHUNK_TYPE: u32 = 0xfffd_0002;
const IMAGE_HEADER_LEN: usize = 36;

#[derive(Debug, Default, Clone, Copy)]
pub struct XcursorPreviewLoader;

impl XcursorPreviewLoader {
    pub fn new() -> Self {
        XcursorPreviewLoader
    }
}

impl PreviewLoader for XcursorPreviewLoader {
    fn load_state(&self, theme: &CursorTheme, state: CursorState) -> Result<Preview, PreviewError> {
        let path = locate_cursor_file(&theme.path, state.candidates())?;
        let bytes = read_capped(&path)?;
        check_layout(&bytes)?;
        let images = parse_xcursor(&bytes)
            .filter(|images| !images.is_empty())
            .ok_or_else(|| PreviewError::Decode("not a valid Xcursor file".to_string()))?;
        build_preview(images)
    }
}

fn locate_cursor_file(theme_path: &Path, names: &[&str]) -> Result<PathBuf, PreviewError> {
    let cursors_dir = fs::canonicalize(theme_path.join("cursors")).map_err(|err| {
        PreviewError::Unavailable(format!(
            "theme has no readable cursors directory ({}): {err}",
            theme_path.display()
        ))
    })?;
    let candidate = names
        .iter()
        .map(|name| cursors_dir.join(name))
        .find(|path| path.exists())
        .ok_or_else(|| {
            PreviewError::Unavailable(format!(
                "none of {} found in {}",
                names.join(", "),
                cursors_dir.display()
            ))
        })?;
    let resolved = fs::canonicalize(&candidate)
        .map_err(|err| PreviewError::Io(format!("{}: {err}", candidate.display())))?;
    // A theme could symlink a cursor name to an arbitrary file elsewhere.
    if !resolved.starts_with(&cursors_dir) {
        return Err(PreviewError::Unavailable(format!(
            "{} resolves outside the theme's cursors directory",
            candidate.display()
        )));
    }
    Ok(resolved)
}

fn read_capped(path: &Path) -> Result<Vec<u8>, PreviewError> {
    let io_err = |err: std::io::Error| PreviewError::Io(format!("{}: {err}", path.display()));
    let file = File::open(path).map_err(io_err)?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(io_err)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(PreviewError::Decode(format!(
            "cursor file exceeds {MAX_FILE_BYTES} bytes"
        )));
    }
    Ok(bytes)
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes(slice.try_into().ok()?))
}

/// The parser trusts header fields when allocating, so reject chunks that
/// claim more data than the file holds before handing the bytes over.
fn check_layout(bytes: &[u8]) -> Result<(), PreviewError> {
    let malformed = || PreviewError::Decode("not a valid Xcursor file".to_string());
    if bytes.get(..4) != Some(b"Xcur") {
        return Err(malformed());
    }
    let header = read_u32(bytes, 4).ok_or_else(malformed)? as usize;
    let ntoc = read_u32(bytes, 12).ok_or_else(malformed)? as usize;
    let toc_end = ntoc
        .checked_mul(TOC_ENTRY_LEN)
        .and_then(|len| len.checked_add(header))
        .filter(|end| header >= HEADER_LEN && *end <= bytes.len())
        .ok_or_else(malformed)?;
    for entry in (header..toc_end).step_by(TOC_ENTRY_LEN) {
        if read_u32(bytes, entry) != Some(IMAGE_CHUNK_TYPE) {
            continue;
        }
        let pos = read_u32(bytes, entry + 8).ok_or_else(malformed)? as usize;
        let width = read_u32(bytes, pos.saturating_add(16)).ok_or_else(malformed)? as usize;
        let height = read_u32(bytes, pos.saturating_add(20)).ok_or_else(malformed)? as usize;
        width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4))
            .and_then(|data| data.checked_add(pos.saturating_add(IMAGE_HEADER_LEN)))
            .filter(|end| *end <= bytes.len())
            .ok_or_else(malformed)?;
    }
    Ok(())
}

fn closest_size(images: &[Image]) -> Option<u32> {
    images
        .iter()
        .map(|image| image.size)
        .min_by_key(|size| (size.abs_diff(TARGET_SIZE), std::cmp::Reverse(*size)))
}

fn build_preview(images: Vec<Image>) -> Result<Preview, PreviewError> {
    let size = closest_size(&images)
        .ok_or_else(|| PreviewError::Decode("cursor file has no images".to_string()))?;
    let selected: Vec<Image> = images
        .into_iter()
        .filter(|image| image.size == size)
        .collect();
    if selected.len() > MAX_FRAMES {
        return Err(PreviewError::Decode(format!(
            "cursor has {} frames, more than the {MAX_FRAMES} supported",
            selected.len()
        )));
    }
    let frames = selected
        .into_iter()
        .map(to_frame)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Preview { frames })
}

fn to_frame(image: Image) -> Result<PreviewFrame, PreviewError> {
    if image.width > MAX_DIMENSION || image.height > MAX_DIMENSION {
        return Err(PreviewError::Decode(format!(
            "frame is {}x{}, larger than {MAX_DIMENSION}x{MAX_DIMENSION}",
            image.width, image.height
        )));
    }
    // Despite its name, the crate's `pixels_rgba` holds the file bytes as
    // stored (B, G, R, A for little-endian ARGB), so swap into RGBA here.
    // Xcursor data is premultiplied alpha and is passed through as is.
    let mut rgba = image.pixels_rgba;
    let (pixels, _) = rgba.as_chunks_mut::<4>();
    for pixel in pixels {
        pixel.swap(0, 2);
    }
    Ok(PreviewFrame {
        width: image.width,
        height: image.height,
        rgba,
        delay_ms: image.delay,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    struct Img {
        nominal: u32,
        width: u32,
        height: u32,
        delay: u32,
        argb: Vec<u32>,
    }

    fn img(nominal: u32, side: u32, delay: u32, fill: u32) -> Img {
        Img {
            nominal,
            width: side,
            height: side,
            delay,
            argb: vec![fill; (side * side) as usize],
        }
    }

    fn build_xcursor(images: &[Img]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"Xcur");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&0x1_0000u32.to_le_bytes());
        out.extend_from_slice(&(images.len() as u32).to_le_bytes());
        let mut pos = 16 + 12 * images.len() as u32;
        for image in images {
            out.extend_from_slice(&0xfffd_0002u32.to_le_bytes());
            out.extend_from_slice(&image.nominal.to_le_bytes());
            out.extend_from_slice(&pos.to_le_bytes());
            pos += 36 + 4 * image.width * image.height;
        }
        for image in images {
            for field in [
                36,
                0xfffd_0002,
                image.nominal,
                1,
                image.width,
                image.height,
                0,
                0,
                image.delay,
            ] {
                out.extend_from_slice(&u32::to_le_bytes(field));
            }
            for pixel in &image.argb {
                out.extend_from_slice(&pixel.to_le_bytes());
            }
        }
        out
    }

    fn theme_in(dir: &TempDir) -> CursorTheme {
        CursorTheme::new("Test", None, dir.path().to_path_buf()).unwrap()
    }

    fn write_cursor(dir: &TempDir, name: &str, bytes: &[u8]) {
        let cursors = dir.path().join("cursors");
        fs::create_dir_all(&cursors).unwrap();
        fs::write(cursors.join(name), bytes).unwrap();
    }

    fn load(dir: &TempDir) -> Result<Preview, PreviewError> {
        XcursorPreviewLoader::new().load(&theme_in(dir))
    }

    #[test]
    fn picks_nominal_size_closest_to_target() {
        let dir = TempDir::new().unwrap();
        let bytes = build_xcursor(&[img(48, 2, 0, 0xff00_0001), img(24, 3, 0, 0xff00_0002)]);
        write_cursor(&dir, "left_ptr", &bytes);
        let preview = load(&dir).unwrap();
        assert_eq!(preview.frames.len(), 1);
        assert_eq!(preview.frames[0].width, 3);
    }

    #[test]
    fn tie_prefers_the_larger_size() {
        let dir = TempDir::new().unwrap();
        let bytes = build_xcursor(&[img(24, 2, 0, 1), img(40, 3, 0, 2)]);
        write_cursor(&dir, "left_ptr", &bytes);
        assert_eq!(load(&dir).unwrap().frames[0].width, 3);
    }

    #[test]
    fn animated_cursor_keeps_all_frames_in_order_with_delays() {
        let dir = TempDir::new().unwrap();
        let bytes = build_xcursor(&[
            img(32, 2, 10, 1),
            img(32, 2, 20, 2),
            img(32, 2, 30, 3),
            img(64, 4, 99, 4),
        ]);
        write_cursor(&dir, "left_ptr", &bytes);
        let delays: Vec<u32> = load(&dir)
            .unwrap()
            .frames
            .iter()
            .map(|f| f.delay_ms)
            .collect();
        assert_eq!(delays, vec![10, 20, 30]);
    }

    #[test]
    fn static_cursor_has_a_single_frame() {
        let dir = TempDir::new().unwrap();
        write_cursor(&dir, "left_ptr", &build_xcursor(&[img(32, 2, 0, 1)]));
        let frame = &load(&dir).unwrap().frames[0];
        assert_eq!((frame.width, frame.height, frame.delay_ms), (2, 2, 0));
        assert_eq!(frame.rgba.len(), 2 * 2 * 4);
    }

    #[test]
    fn pixel_round_trips_to_rgba_bytes() {
        let dir = TempDir::new().unwrap();
        // A=0x80, R=0xFF, G=0x40, B=0x20
        let bytes = build_xcursor(&[img(32, 1, 0, 0x80FF_4020)]);
        write_cursor(&dir, "left_ptr", &bytes);
        assert_eq!(
            load(&dir).unwrap().frames[0].rgba,
            vec![0xFF, 0x40, 0x20, 0x80]
        );
    }

    #[test]
    fn falls_back_through_candidate_names() {
        let dir = TempDir::new().unwrap();
        write_cursor(&dir, "arrow", &build_xcursor(&[img(32, 2, 0, 1)]));
        write_cursor(&dir, "default", &build_xcursor(&[img(32, 3, 0, 1)]));
        assert_eq!(load(&dir).unwrap().frames[0].width, 3);
    }

    #[test]
    fn errors_when_no_candidate_exists() {
        let dir = TempDir::new().unwrap();
        write_cursor(&dir, "watch", &build_xcursor(&[img(32, 2, 0, 1)]));
        assert!(matches!(load(&dir), Err(PreviewError::Unavailable(_))));
    }

    #[test]
    fn errors_when_cursors_dir_is_missing() {
        let dir = TempDir::new().unwrap();
        assert!(matches!(load(&dir), Err(PreviewError::Unavailable(_))));
    }

    #[test]
    fn follows_symlink_inside_cursors_dir() {
        let dir = TempDir::new().unwrap();
        write_cursor(&dir, "arrow", &build_xcursor(&[img(32, 3, 0, 1)]));
        symlink("arrow", dir.path().join("cursors/left_ptr")).unwrap();
        assert_eq!(load(&dir).unwrap().frames[0].width, 3);
    }

    #[test]
    fn rejects_symlink_escaping_cursors_dir() {
        let theme = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let target = outside.path().join("secret");
        fs::write(&target, build_xcursor(&[img(32, 2, 0, 1)])).unwrap();
        fs::create_dir_all(theme.path().join("cursors")).unwrap();
        symlink(&target, theme.path().join("cursors/left_ptr")).unwrap();
        assert!(matches!(load(&theme), Err(PreviewError::Unavailable(_))));
    }

    #[test]
    fn rejects_files_over_the_size_cap() {
        let dir = TempDir::new().unwrap();
        write_cursor(&dir, "left_ptr", &vec![0u8; (MAX_FILE_BYTES + 1) as usize]);
        assert!(matches!(load(&dir), Err(PreviewError::Decode(_))));
    }

    #[test]
    fn rejects_frames_larger_than_256() {
        let dir = TempDir::new().unwrap();
        write_cursor(&dir, "left_ptr", &build_xcursor(&[img(32, 257, 0, 1)]));
        assert!(matches!(load(&dir), Err(PreviewError::Decode(_))));
    }

    #[test]
    fn accepts_frames_of_exactly_256() {
        let dir = TempDir::new().unwrap();
        write_cursor(&dir, "left_ptr", &build_xcursor(&[img(32, 256, 0, 1)]));
        assert_eq!(load(&dir).unwrap().frames[0].width, 256);
    }

    #[test]
    fn rejects_more_than_64_frames() {
        let dir = TempDir::new().unwrap();
        let images: Vec<Img> = (0..65).map(|_| img(32, 1, 1, 1)).collect();
        write_cursor(&dir, "left_ptr", &build_xcursor(&images));
        assert!(matches!(load(&dir), Err(PreviewError::Decode(_))));
    }

    #[test]
    fn rejects_garbage_and_truncated_files() {
        let dir = TempDir::new().unwrap();
        write_cursor(&dir, "left_ptr", b"definitely not a cursor");
        assert!(matches!(load(&dir), Err(PreviewError::Decode(_))));
        let mut truncated = build_xcursor(&[img(32, 4, 0, 1)]);
        truncated.truncate(truncated.len() - 8);
        write_cursor(&dir, "left_ptr", &truncated);
        assert!(matches!(load(&dir), Err(PreviewError::Decode(_))));
    }

    #[test]
    fn rejects_headers_claiming_more_than_the_file_holds() {
        let dir = TempDir::new().unwrap();
        let mut huge_toc = build_xcursor(&[img(32, 1, 0, 1)]);
        huge_toc[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        write_cursor(&dir, "left_ptr", &huge_toc);
        assert!(matches!(load(&dir), Err(PreviewError::Decode(_))));

        let mut huge_image = build_xcursor(&[img(32, 1, 0, 1)]);
        let pos = 16 + 12;
        huge_image[pos + 16..pos + 20].copy_from_slice(&0x7fffu32.to_le_bytes());
        huge_image[pos + 20..pos + 24].copy_from_slice(&0x7fffu32.to_le_bytes());
        write_cursor(&dir, "left_ptr", &huge_image);
        assert!(matches!(load(&dir), Err(PreviewError::Decode(_))));
    }

    #[test]
    fn rejects_a_file_with_no_images() {
        let dir = TempDir::new().unwrap();
        write_cursor(&dir, "left_ptr", &build_xcursor(&[]));
        assert!(matches!(load(&dir), Err(PreviewError::Decode(_))));
    }

    // --- cursor states ---

    fn load_state(dir: &TempDir, state: CursorState) -> Result<Preview, PreviewError> {
        XcursorPreviewLoader::new().load_state(&theme_in(dir), state)
    }

    #[test]
    fn load_is_the_default_state() {
        let dir = TempDir::new().unwrap();
        write_cursor(&dir, "default", &build_xcursor(&[img(32, 3, 0, 1)]));
        write_cursor(&dir, "hand2", &build_xcursor(&[img(32, 5, 0, 1)]));
        assert_eq!(
            load(&dir).unwrap(),
            load_state(&dir, CursorState::Default).unwrap()
        );
    }

    #[test]
    fn each_state_resolves_to_its_own_cursor_file() {
        let dir = TempDir::new().unwrap();
        for (side, state) in (1..).zip(CursorState::ALL) {
            write_cursor(
                &dir,
                state.candidates()[0],
                &build_xcursor(&[img(32, side, 0, 1)]),
            );
        }
        for (side, state) in (1..).zip(CursorState::ALL) {
            assert_eq!(load_state(&dir, state).unwrap().frames[0].width, side);
        }
    }

    #[test]
    fn pointer_resolves_through_the_hand2_alias() {
        let dir = TempDir::new().unwrap();
        write_cursor(&dir, "hand2", &build_xcursor(&[img(32, 4, 0, 1)]));
        write_cursor(&dir, "left_ptr", &build_xcursor(&[img(32, 2, 0, 1)]));
        assert_eq!(
            load_state(&dir, CursorState::Pointer).unwrap().frames[0].width,
            4
        );
    }

    #[test]
    fn the_first_existing_candidate_wins() {
        let dir = TempDir::new().unwrap();
        write_cursor(&dir, "hand", &build_xcursor(&[img(32, 2, 0, 1)]));
        write_cursor(&dir, "hand1", &build_xcursor(&[img(32, 3, 0, 1)]));
        write_cursor(&dir, "pointer", &build_xcursor(&[img(32, 4, 0, 1)]));
        assert_eq!(
            load_state(&dir, CursorState::Pointer).unwrap().frames[0].width,
            4
        );
    }

    #[test]
    fn a_missing_state_is_unavailable_and_names_the_candidates() {
        let dir = TempDir::new().unwrap();
        write_cursor(&dir, "left_ptr", &build_xcursor(&[img(32, 2, 0, 1)]));
        let err = load_state(&dir, CursorState::Text).unwrap_err();
        assert!(
            matches!(&err, PreviewError::Unavailable(m) if m.contains("xterm")),
            "{err}"
        );
    }

    #[test]
    fn animated_states_keep_every_frame() {
        let dir = TempDir::new().unwrap();
        let frames: Vec<Img> = (0..12).map(|_| img(32, 2, 40, 1)).collect();
        write_cursor(&dir, "left_ptr_watch", &build_xcursor(&frames));
        let preview = load_state(&dir, CursorState::Progress).unwrap();
        assert_eq!(preview.frames.len(), 12);
    }

    #[test]
    fn limits_apply_to_every_state() {
        let dir = TempDir::new().unwrap();
        write_cursor(&dir, "xterm", &build_xcursor(&[img(32, 257, 0, 1)]));
        assert!(matches!(
            load_state(&dir, CursorState::Text),
            Err(PreviewError::Decode(_))
        ));
        let many: Vec<Img> = (0..65).map(|_| img(32, 1, 1, 1)).collect();
        write_cursor(&dir, "watch", &build_xcursor(&many));
        assert!(matches!(
            load_state(&dir, CursorState::Wait),
            Err(PreviewError::Decode(_))
        ));
        write_cursor(&dir, "help", &vec![0u8; (MAX_FILE_BYTES + 1) as usize]);
        assert!(matches!(
            load_state(&dir, CursorState::Help),
            Err(PreviewError::Decode(_))
        ));
    }

    #[test]
    fn a_state_symlink_escaping_the_cursors_dir_is_rejected() {
        let theme = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let target = outside.path().join("secret");
        fs::write(&target, build_xcursor(&[img(32, 2, 0, 1)])).unwrap();
        fs::create_dir_all(theme.path().join("cursors")).unwrap();
        symlink(&target, theme.path().join("cursors/pointer")).unwrap();
        assert!(matches!(
            load_state(&theme, CursorState::Pointer),
            Err(PreviewError::Unavailable(_))
        ));
    }
}
