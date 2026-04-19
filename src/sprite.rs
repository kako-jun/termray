use crate::camera::{Camera, projection_center_y};
use crate::framebuffer::{Color, Framebuffer};
use crate::map::HeightMap;
use crate::math::normalize_angle;
use crate::ray::RayHit;

#[derive(Debug, Clone)]
pub struct Sprite {
    pub x: f64,
    pub y: f64,
    pub sprite_type: u8,
}

/// Per-sprite screen-space data produced by [`project_sprites`].
///
/// `screen_y_feet` is the pre-computed screen y (in framebuffer pixels) of
/// the sprite's ground-contact point after sampling the floor under it with
/// bilinear interpolation and accounting for the camera's pitch. It replaces
/// the fixed "sprite sits at `fb_height / 2`" assumption used in pre-v0.3
/// termray, so sprites on sloped floors actually stand on the slope.
#[derive(Debug, Clone)]
pub struct SpriteRenderResult {
    pub screen_x: i32,
    /// Base sprite height in framebuffer pixels, computed from the vertical
    /// projection (`fb_height / distance` via `focal_y = fb_height / 2`).
    ///
    /// v0.3 aligns sprite vertical scaling with the floor renderer's
    /// vertical FOV baseline. Previously this was `screen_width / distance`,
    /// which coupled vertical sprite size to horizontal FOV and left
    /// sprites taller than they should be on wide / short framebuffers.
    ///
    /// [`render_sprites`] multiplies this by the per-type `height_scale` to
    /// produce the actual on-screen sprite height.
    pub screen_height: i32,
    pub distance: f64,
    pub sprite_type: u8,
    /// Screen y (framebuffer pixels, fractional) of the sprite's feet after
    /// projecting it onto the floor surface via [`HeightMap`] bilinear
    /// sampling and applying the camera pitch horizon shift. Used by
    /// [`render_sprites`] as the anchor row from which the sprite art is
    /// drawn upward. Kept as `f64` so sub-pixel pitch transitions don't
    /// alias; quantized to `i32` inside [`render_sprites`] at pixel-write
    /// time.
    pub screen_y_feet: f64,
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
///
/// The `heights` map is sampled bilinearly at each sprite's `(x, y)` to
/// determine the world-space elevation of its feet, so sprites sitting on a
/// slope inherit the slope. `camera.pitch` contributes to the vertical
/// horizon shift via the same `center_y = fb_height/2 + tan(pitch) * focal_px`
/// convention used by walls and the floor renderer — sprites therefore track
/// pitch without any per-sprite vertical math in the caller.
///
/// Sprites closer than [`crate::MIN_PROJECTION_DISTANCE`] are dropped to
/// avoid absurd magnification near the camera; the same constant gates
/// [`crate::label::project_labels`].
pub fn project_sprites(
    sprites: &[Sprite],
    camera: &Camera,
    heights: &dyn HeightMap,
    screen_width: usize,
    screen_height: usize,
) -> Vec<SpriteRenderResult> {
    // Projection anchors: center_y = fb/2 + tan(pitch)*focal_px,
    // focal_y = fb_height / 2 (matching the floor renderer's vertical-FOV
    // baseline). Sprite vertical scale therefore shares the same convention
    // as walls / floor — pre-v0.3 sprite.rs accidentally used `screen_width`
    // here, which coupled vertical sprite size to horizontal FOV.
    let center_y = projection_center_y(screen_width, screen_height, camera);
    let focal_y = screen_height as f64 / 2.0;
    let fov = camera.fov;
    let camera_x = camera.x;
    let camera_y = camera.y;
    let camera_angle = camera.angle;

    let mut results: Vec<SpriteRenderResult> = sprites
        .iter()
        .filter_map(|s| {
            let dx = s.x - camera_x;
            let dy = s.y - camera_y;
            let distance = (dx * dx + dy * dy).sqrt();

            if distance < crate::MIN_PROJECTION_DISTANCE {
                return None;
            }

            let sprite_angle = dy.atan2(dx);
            let angle_diff = normalize_angle(sprite_angle - camera_angle + std::f64::consts::PI)
                - std::f64::consts::PI;

            if angle_diff.abs() > fov * 0.6 {
                return None;
            }

            let screen_x = ((angle_diff + fov / 2.0) / fov * screen_width as f64) as i32;

            // Bilinear-sample the floor under the sprite's footprint so it
            // plants on whatever slope is at (s.x, s.y). `cell_heights`
            // handles out-of-bounds by returning flat defaults.
            let cell_x = s.x.floor() as i32;
            let cell_y = s.y.floor() as i32;
            let u = s.x - cell_x as f64;
            let v = s.y - cell_y as f64;
            let floor_h = heights
                .cell_heights(cell_x, cell_y)
                .sample_floor(u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));

            // Project the feet: screen_y = center_y + focal_y * (cam.z - floor_h) / d
            let screen_y_feet_f = center_y + focal_y * (camera.z - floor_h) / distance;

            // Base vertical height in pixels = focal_y * 2 / distance = fb_h / distance.
            // Vertical scaling is what we want: walls, floors, and sprites all
            // share the fb_height-based vertical FOV baseline.
            let base_height = (screen_height as f64 / distance) as i32;

            Some(SpriteRenderResult {
                screen_x,
                // screen_height is the base size; `render_sprites` scales by per-type `height_scale`.
                screen_height: base_height,
                distance,
                sprite_type: s.sprite_type,
                screen_y_feet: screen_y_feet_f,
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
    let _fb_height = fb.height() as f64;

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

        // Anchor the sprite's feet to the pre-projected ground row, then
        // grow upward by `sprite_h`. Applies the per-type float offset on
        // top of the ground anchor so floating sprites still work. Quantize
        // the f64 feet position here at the pixel-write boundary so sub-pixel
        // pitch/slope motion doesn't double-round.
        let feet_i = spr.screen_y_feet as i32;
        let float_offset = (sprite_h as f64 * def.float_offset_scale) as i32;
        let y_top = feet_i - sprite_h - float_offset;

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
