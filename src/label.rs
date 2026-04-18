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

use crate::framebuffer::{Color, Framebuffer};
use crate::math::normalize_angle;
use crate::ray::RayHit;

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
    /// World height used for vertical projection. Preserved from the source
    /// [`Label`] because [`render_labels`] re-projects each label's vertical
    /// position at paint time (see the caveat on [`project_labels`]).
    pub world_height: f64,
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
/// Labels behind the camera or outside `±0.6 * fov` are culled, matching the
/// [`crate::sprite::project_sprites`] convention. Labels closer than 0.3 world
/// units are also skipped to avoid absurd on-screen magnification right in
/// front of the camera.
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
    camera_x: f64,
    camera_y: f64,
    camera_angle: f64,
    fov: f64,
    screen_width: usize,
) -> Vec<ProjectedLabel> {
    let mut results: Vec<ProjectedLabel> = labels
        .iter()
        .filter_map(|lbl| {
            let dx = lbl.x - camera_x;
            let dy = lbl.y - camera_y;
            let distance = (dx * dx + dy * dy).sqrt();

            if distance < 0.3 {
                return None;
            }

            let label_angle = dy.atan2(dx);
            let angle_diff = normalize_angle(label_angle - camera_angle + std::f64::consts::PI)
                - std::f64::consts::PI;

            if angle_diff.abs() > fov * 0.6 {
                return None;
            }

            let screen_x = ((angle_diff + fov / 2.0) / fov * screen_width as f64) as i32;

            let lines = match lbl.max_chars {
                Some(n) => wrap_text(&lbl.text, n),
                None => {
                    if lbl.text.is_empty() {
                        Vec::new()
                    } else {
                        vec![lbl.text.clone()]
                    }
                }
            };

            if lines.is_empty() {
                return None;
            }

            Some(ProjectedLabel {
                screen_x,
                distance,
                lines,
                color: lbl.color,
                background: lbl.background,
                world_height: lbl.world_height,
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
    let center_y = fb_h / 2;

    for lbl in projected {
        if lbl.distance > max_depth || lbl.lines.is_empty() {
            continue;
        }

        // Vertical projection matches sprite.rs: center_y - (screen_width / distance) * world_height.
        // `screen_width` here is the framebuffer width (same as the sprite path).
        let baseline_y = center_y - ((fb_w as f64 / lbl.distance) * lbl.world_height) as i32;

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
                        fb.blend_pixel(px as usize, py as usize, bg, 0.6);
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
