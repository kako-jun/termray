//! World-anchored text labels.
//!
//! A [`Label`] is an independent world-space entity — pair it with a
//! [`crate::sprite::Sprite`] at the same `(x, y)` to get an "icon with caption"
//! composition (e.g. a file icon + file name for friendly-filer).
//!
//! The glyph renderer is pluggable via the [`GlyphRenderer`] trait. The
//! bundled [`Font8x8`] implementation covers `basic_latin` (0x20..=0x7E).
//! Applications that need richer scripts (CJK etc.) supply their own
//! `GlyphRenderer`.

use font8x8::legacy::BASIC_LEGACY;

use crate::camera::{Camera, projection_center_y};
use crate::framebuffer::{Color, Framebuffer};
use crate::map::HeightMap;
use crate::math::normalize_angle;
use crate::ray::RayHit;

/// Fraction of the FOV (measured from the centerline) inside which labels survive
/// culling. Matches the sprite convention so labels fade in/out at the same
/// screen edges as the sprites they caption.
const FOV_CULL_FRACTION: f64 = 0.6;

/// Minimum distance (world units) at which a label is still rendered. Labels
/// closer than this are skipped to avoid absurd on-screen magnification right
/// in front of the camera — matches the sprite near-cut.
pub const MIN_LABEL_DISTANCE: f64 = 0.3;

/// Alpha used when blending a label's optional background rectangle into the
/// framebuffer. Tuned for readability against both light and dark walls.
const BACKGROUND_ALPHA: f64 = 0.6;

/// A world-anchored text label rendered as fixed-size glyphs in the framebuffer.
#[derive(Debug, Clone)]
pub struct Label {
    pub text: String,
    pub x: f64,
    pub y: f64,
    /// Height above the floor in world units. `1.0` is roughly one wall-height.
    /// Typical values: `0.8` for a label just above a head-height sprite.
    pub world_height: f64,
    pub color: Color,
    /// Optional semi-transparent backing box behind the text for readability.
    pub background: Option<Color>,
    /// If set, wrap text at this many characters (word-wrap on whitespace).
    pub max_chars: Option<usize>,
}

impl Default for Label {
    /// Sensible defaults for a label hovering just above a head-height sprite:
    /// empty text, origin `(0, 0)`, `world_height = 0.8`, white, no background,
    /// no wrapping.
    fn default() -> Self {
        Self {
            text: String::new(),
            x: 0.0,
            y: 0.0,
            world_height: 0.8,
            color: Color::rgb(255, 255, 255),
            background: None,
            max_chars: None,
        }
    }
}

/// Pluggable glyph renderer — lets applications inject their own bitmap fonts
/// (e.g. a CJK-capable renderer for friendly-filer with Japanese filenames).
pub trait GlyphRenderer {
    fn glyph_width(&self) -> usize;
    fn glyph_height(&self) -> usize;
    /// Draw `ch` at top-left `(x, y)` in the framebuffer. Unsupported characters
    /// may render as nothing or a fallback glyph — that choice is up to the impl.
    fn draw_glyph(&self, fb: &mut Framebuffer, x: i32, y: i32, ch: char, color: Color);
}

/// 8×8 monochrome ASCII-Latin font backed by the `font8x8` crate.
///
/// Covers the `basic_latin` range (0x20..=0x7E). Characters outside that range
/// draw nothing. To ship richer scripts (CJK, etc.) supply your own
/// [`GlyphRenderer`] implementation.
pub struct Font8x8;

impl GlyphRenderer for Font8x8 {
    fn glyph_width(&self) -> usize {
        8
    }

    fn glyph_height(&self) -> usize {
        8
    }

    fn draw_glyph(&self, fb: &mut Framebuffer, x: i32, y: i32, ch: char, color: Color) {
        // Only basic_latin printable range is supported.
        let code = ch as u32;
        if !(0x20..=0x7E).contains(&code) {
            return;
        }
        // `BASIC_LEGACY` is a flat `[[u8; 8]; 128]` laid out at U+0000..=U+007F
        // — for our already-validated basic_latin range we can index directly.
        // Using the `legacy` submodule (always public, no feature flag needed)
        // avoids pulling in the ~24 KB unicode tables.
        let bitmap: [u8; 8] = BASIC_LEGACY[code as usize];
        for (row, byte) in bitmap.iter().enumerate() {
            for col in 0i32..8 {
                // font8x8 bitmap convention: bit 0 (LSB) = leftmost column.
                if byte & (1 << col) != 0 {
                    let px = x + col;
                    let py = y + row as i32;
                    if px >= 0 && py >= 0 {
                        fb.set_pixel(px as usize, py as usize, color);
                    }
                }
            }
        }
    }
}

/// Output of [`project_labels`] — one per visible label, in paint order (far to near).
#[derive(Debug, Clone)]
pub struct ProjectedLabel {
    pub screen_x: i32,
    pub distance: f64,
    /// Post-wrap lines. Caller need not re-wrap at render time.
    pub lines: Vec<String>,
    pub color: Color,
    pub background: Option<Color>,
    /// Pre-computed screen y (framebuffer pixels) of the baseline the
    /// renderer draws the first text line against. Projected in
    /// [`project_labels`] from the source [`Label`]'s `world_height` plus
    /// the bilinear-sampled floor height under the label anchor, with the
    /// camera pitch horizon shift applied — matches the sprite projection
    /// so a label attached to a sprite tracks it on sloped ground.
    pub screen_y_baseline: i32,
}

/// Greedy word-wrap on ASCII whitespace. Words longer than `max` are hard-split
/// at `max`-character boundaries.
fn wrap_text(text: &str, max: usize) -> Vec<String> {
    if max == 0 {
        return Vec::new();
    }
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();

    let words: Vec<&str> = text.split_ascii_whitespace().collect();
    if words.is_empty() {
        return lines;
    }

    for word in words {
        // Hard-split oversize words into chunks of at most `max` chars.
        let chunks = hard_split(word, max);
        for (i, chunk) in chunks.iter().enumerate() {
            // Only the first chunk may join the current line; subsequent chunks
            // form their own lines (matching typical word-wrap behavior).
            if i == 0 {
                if current.is_empty() {
                    current.push_str(chunk);
                } else if current.chars().count() + 1 + chunk.chars().count() <= max {
                    current.push(' ');
                    current.push_str(chunk);
                } else {
                    lines.push(std::mem::take(&mut current));
                    current.push_str(chunk);
                }
            } else {
                if !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                }
                current.push_str(chunk);
            }
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn hard_split(word: &str, max: usize) -> Vec<String> {
    // Defensive: callers currently only pass non-empty words (since
    // `split_ascii_whitespace` skips empty items), but guard anyway.
    if word.is_empty() {
        return Vec::new();
    }
    if word.chars().count() <= max {
        return vec![word.to_string()];
    }
    let mut out: Vec<String> = Vec::new();
    let mut buf = String::new();
    let mut count = 0usize;
    for ch in word.chars() {
        buf.push(ch);
        count += 1;
        if count == max {
            out.push(std::mem::take(&mut buf));
            count = 0;
        }
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

/// Project labels into screen space (FOV-cull + distance-cull + word-wrap).
///
/// Labels behind the camera or outside `±FOV_CULL_FRACTION * fov` are culled,
/// matching the [`crate::sprite::project_sprites`] convention. Labels closer
/// than [`MIN_LABEL_DISTANCE`] world units are also skipped to avoid absurd
/// on-screen magnification right in front of the camera.
///
/// Results are sorted far-to-near so caller rendering follows the painter's
/// algorithm — matches [`crate::sprite::project_sprites`].
///
/// # Caveat
///
/// The `screen_width` passed here must match the width of the framebuffer
/// later passed to [`render_labels`]: the renderer re-projects each label's
/// vertical position and assumes the same horizontal FOV scaling. Passing
/// mismatched widths will produce vertically misaligned text.
pub fn project_labels(
    labels: &[Label],
    camera: &Camera,
    heights: &dyn HeightMap,
    screen_width: usize,
    screen_height: usize,
) -> Vec<ProjectedLabel> {
    // Vertical projection anchors: matches `project_sprites` so an icon +
    // caption pair shares the same horizon shift under pitch / slope.
    let center_y = projection_center_y(screen_width, screen_height, camera);
    let focal_y = screen_height as f64 / 2.0;
    let fov = camera.fov;
    let camera_x = camera.x;
    let camera_y = camera.y;
    let camera_angle = camera.angle;

    let mut results: Vec<ProjectedLabel> = labels
        .iter()
        .filter_map(|lbl| {
            let dx = lbl.x - camera_x;
            let dy = lbl.y - camera_y;
            let distance = (dx * dx + dy * dy).sqrt();

            if distance < MIN_LABEL_DISTANCE {
                return None;
            }

            let label_angle = dy.atan2(dx);
            let angle_diff = normalize_angle(label_angle - camera_angle + std::f64::consts::PI)
                - std::f64::consts::PI;

            if angle_diff.abs() > fov * FOV_CULL_FRACTION {
                return None;
            }

            let screen_x = ((angle_diff + fov / 2.0) / fov * screen_width as f64) as i32;

            let lines = match lbl.max_chars {
                Some(n) => wrap_text(&lbl.text, n),
                None => {
                    // Match `wrap_text`'s semantics: a label whose text has no
                    // visible (non-whitespace) characters is dropped entirely,
                    // rather than rendering an empty background rectangle.
                    if lbl.text.chars().all(char::is_whitespace) {
                        Vec::new()
                    } else {
                        vec![lbl.text.clone()]
                    }
                }
            };

            if lines.is_empty() {
                return None;
            }

            // Sample the floor under the label's world anchor so the
            // `world_height` offset is measured from the actual ground at
            // `(lbl.x, lbl.y)` — matching how a sprite at the same
            // `(x, y)` is planted (see `project_sprites`).
            let cell_x = lbl.x.floor() as i32;
            let cell_y = lbl.y.floor() as i32;
            let u = lbl.x - cell_x as f64;
            let v = lbl.y - cell_y as f64;
            let floor_h = heights
                .cell_heights(cell_x, cell_y)
                .sample_floor(u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));

            // Projected baseline y: the label anchor is at world z =
            // floor_h + world_height, so
            //   y = center_y + focal_y * (camera.z - (floor_h + world_height)) / d.
            let baseline_y =
                center_y + focal_y * (camera.z - (floor_h + lbl.world_height)) / distance;

            Some(ProjectedLabel {
                screen_x,
                distance,
                lines,
                color: lbl.color,
                background: lbl.background,
                screen_y_baseline: baseline_y as i32,
            })
        })
        .collect();

    results.sort_by(|a, b| b.distance.total_cmp(&a.distance));
    results
}

/// Render projected labels into the framebuffer.
///
/// - Glyphs are drawn at the font's native pixel size (no distance scaling)
///   so labels remain readable at all depths.
/// - Labels whose distance exceeds `max_depth` are skipped.
/// - Occlusion has two granularities: glyphs use glyph-level occlusion
///   (skipped wholesale if any of their columns are hidden by a wall), while
///   optional background boxes are blended per-column. This gives visually
///   clean glyph edges at wall corners without the background rectangle
///   leaking past them.
///
/// # Caveat
///
/// `fb.width()` must match the `screen_width` that was passed to
/// [`project_labels`] for `projected` — the renderer re-projects each label's
/// vertical position and assumes the same horizontal FOV scaling. Passing a
/// mismatched framebuffer will produce vertically misaligned text.
pub fn render_labels(
    fb: &mut Framebuffer,
    projected: &[ProjectedLabel],
    rays: &[Option<RayHit>],
    font: &dyn GlyphRenderer,
    max_depth: f64,
) {
    let gw = font.glyph_width() as i32;
    let gh = font.glyph_height() as i32;
    let line_step = gh + 1; // 1 px leading

    let fb_w = fb.width() as i32;
    let fb_h = fb.height() as i32;

    for lbl in projected {
        if lbl.distance > max_depth || lbl.lines.is_empty() {
            continue;
        }

        // Use the baseline pre-computed in `project_labels` — it already
        // folds in the camera pitch and the sampled floor under the label.
        let baseline_y = lbl.screen_y_baseline;

        for (li, line) in lbl.lines.iter().enumerate() {
            let line_chars: Vec<char> = line.chars().collect();
            let line_w = line_chars.len() as i32 * gw;
            // Per-line horizontal centering relative to the block's centered origin.
            let line_x_left = lbl.screen_x - line_w / 2;
            let y_top = baseline_y + li as i32 * line_step;

            // 1. Draw background (if any) with per-column depth test.
            if let Some(bg) = lbl.background {
                for col_off in 0..line_w {
                    let px = line_x_left + col_off;
                    if px < 0 || px >= fb_w {
                        continue;
                    }
                    // Depth test: hide background columns occluded by walls.
                    if let Some(Some(hit)) = rays.get(px as usize) {
                        if lbl.distance > hit.distance {
                            continue;
                        }
                    }
                    for row_off in 0..gh {
                        let py = y_top + row_off;
                        if py < 0 || py >= fb_h {
                            continue;
                        }
                        fb.blend_pixel(px as usize, py as usize, bg, BACKGROUND_ALPHA);
                    }
                }
            }

            // 2. Draw glyphs. Because the `GlyphRenderer` trait draws a whole
            //    glyph at once, we apply occlusion at glyph granularity: skip
            //    the glyph if any of its columns is behind a wall. With 8-px
            //    glyphs this gives visually clean edges — labels peeking past
            //    a wall corner drop at the glyph boundary rather than
            //    tearing. (Per-pixel occlusion of the background rectangle
            //    above still happens at column granularity.)
            for (ci, ch) in line_chars.iter().enumerate() {
                let glyph_x = line_x_left + ci as i32 * gw;

                let mut all_visible = true;
                for col in 0..gw {
                    let px = glyph_x + col;
                    if px < 0 || px >= fb_w {
                        continue;
                    }
                    if let Some(Some(hit)) = rays.get(px as usize) {
                        if lbl.distance > hit.distance {
                            all_visible = false;
                            break;
                        }
                    }
                }
                if all_visible {
                    font.draw_glyph(fb, glyph_x, y_top, *ch, lbl.color);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_hard_split_oversize_word() {
        let lines = wrap_text("abcdefghij", 4);
        assert_eq!(lines, vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn wrap_empty() {
        let lines = wrap_text("", 10);
        assert!(lines.is_empty());
    }

    #[test]
    fn wrap_greedy_words() {
        let lines = wrap_text("hello world foo", 8);
        assert_eq!(lines, vec!["hello", "world", "foo"]);
    }

    #[test]
    fn wrap_last_word_oversize() {
        let lines = wrap_text("hi superlongword", 6);
        assert_eq!(lines, vec!["hi", "superl", "ongwor", "d"]);
    }
}
