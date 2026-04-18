use crate::framebuffer::{Color, Framebuffer};
use crate::math::normalize_angle;
use crate::ray::RayHit;

#[derive(Debug, Clone)]
pub struct Sprite {
    pub x: f64,
    pub y: f64,
    pub sprite_type: u8,
}

#[derive(Debug, Clone)]
pub struct SpriteRenderResult {
    pub screen_x: i32,
    pub screen_height: i32,
    pub distance: f64,
    pub sprite_type: u8,
}

/// ASCII-art pattern used to render a sprite type.
///
/// Each row of `pattern` describes one scanline of the canonical art:
///   - `#` = primary color
///   - `+` = shadow / secondary color
///   - any other char = transparent
///
/// `height_scale` controls vertical size relative to the "wall height" scale.
/// A value of `0.25` yields a sprite about a quarter of a full wall tall.
/// `float_offset_scale` lifts the sprite above the floor (0.0 = sits on floor).
#[derive(Debug, Clone)]
pub struct SpriteDef {
    pub pattern: &'static [&'static str],
    pub height_scale: f64,
    pub float_offset_scale: f64,
}

/// Pluggable sprite art source.
pub trait SpriteArt {
    fn art(&self, sprite_type: u8) -> Option<&SpriteDef>;
    /// Per-type color used when rendering `#` pixels. `+` pixels use a darkened version.
    fn color(&self, sprite_type: u8) -> Color;
}

/// Project sprites into screen space, sorted far-to-near (painter's algorithm).
pub fn project_sprites(
    sprites: &[Sprite],
    camera_x: f64,
    camera_y: f64,
    camera_angle: f64,
    fov: f64,
    screen_width: usize,
) -> Vec<SpriteRenderResult> {
    let mut results: Vec<SpriteRenderResult> = sprites
        .iter()
        .filter_map(|s| {
            let dx = s.x - camera_x;
            let dy = s.y - camera_y;
            let distance = (dx * dx + dy * dy).sqrt();

            if distance < 0.3 {
                return None;
            }

            let sprite_angle = dy.atan2(dx);
            let angle_diff = normalize_angle(sprite_angle - camera_angle + std::f64::consts::PI)
                - std::f64::consts::PI;

            if angle_diff.abs() > fov * 0.6 {
                return None;
            }

            let screen_x = ((angle_diff + fov / 2.0) / fov * screen_width as f64) as i32;

            Some(SpriteRenderResult {
                screen_x,
                // screen_height is the base size; `render_sprites` scales by per-type `height_scale`.
                screen_height: (screen_width as f64 / distance) as i32,
                distance,
                sprite_type: s.sprite_type,
            })
        })
        .collect();

    results.sort_by(|a, b| b.distance.total_cmp(&a.distance));
    results
}

/// Render projected sprites into the framebuffer using the supplied art source.
///
/// Sprites are depth-tested against `rays` so they are hidden behind walls.
pub fn render_sprites(
    fb: &mut Framebuffer,
    projected: &[SpriteRenderResult],
    rays: &[Option<RayHit>],
    art: &dyn SpriteArt,
    max_depth: f64,
) {
    let fb_height = fb.height() as f64;

    for spr in projected {
        let Some(def) = art.art(spr.sprite_type) else {
            continue;
        };
        let pat = def.pattern;
        let pat_h = pat.len();
        let pat_w = pat.first().map_or(0, |r| r.len());

        if pat_h == 0 || pat_w == 0 {
            continue;
        }

        let brightness = (1.0 - spr.distance / max_depth).max(0.1);
        let base_color = art.color(spr.sprite_type);
        let color = base_color.darken(brightness);
        let shadow_color = base_color.darken(brightness * 0.5);

        // Scale the generic screen_height by this type's own vertical scale.
        let sprite_h = (spr.screen_height as f64 * def.height_scale) as i32;
        let sprite_w = sprite_h * pat_w as i32 / pat_h.max(1) as i32;

        let center_y = (fb_height / 2.0) as i32;
        let float_offset = (sprite_h as f64 * def.float_offset_scale) as i32;
        let y_top = center_y - sprite_h / 4 - float_offset;

        let x_left = spr.screen_x - sprite_w / 2;

        for sx in 0..sprite_w {
            let screen_x = x_left + sx;
            if screen_x < 0 || screen_x >= fb.width() as i32 {
                continue;
            }
            let col = screen_x as usize;

            // Depth test against walls
            if let Some(Some(hit)) = rays.get(col) {
                if spr.distance > hit.distance {
                    continue;
                }
            }

            let pat_col = ((sx as f64 / sprite_w as f64) * pat_w as f64) as usize;
            let pat_col = pat_col.min(pat_w - 1);

            for sy in 0..sprite_h {
                let screen_y = y_top + sy;
                if screen_y < 0 || screen_y >= fb.height() as i32 {
                    continue;
                }

                let pat_row = ((sy as f64 / sprite_h as f64) * pat_h as f64) as usize;
                let pat_row = pat_row.min(pat_h - 1);

                let ch = pat[pat_row]
                    .as_bytes()
                    .get(pat_col)
                    .copied()
                    .unwrap_or(b'.');
                match ch {
                    b'#' => fb.set_pixel(col, screen_y as usize, color),
                    b'+' => fb.set_pixel(col, screen_y as usize, shadow_color),
                    _ => {}
                }
            }
        }
    }
}
