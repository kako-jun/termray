use crate::camera::Camera;
use crate::framebuffer::{Color, Framebuffer};
use crate::map::{HeightMap, TILE_VOID, TileType};
use crate::ray::{HitSide, RayHit};

/// Pluggable wall texture source.
///
/// Called once per rendered wall pixel. Implementations decide what each tile ID
/// looks like. `tile_hash` is derived from the map coordinates so applications
/// can generate per-tile variation without tracking it themselves.
pub trait WallTexturer {
    fn sample_wall(
        &self,
        tile: TileType,
        wall_x: f64,
        wall_y: f64,
        side: HitSide,
        brightness: f64,
        tile_hash: u32,
    ) -> Color;
}

impl<F> WallTexturer for F
where
    F: Fn(TileType, f64, f64, HitSide, f64, u32) -> Color,
{
    fn sample_wall(
        &self,
        tile: TileType,
        wall_x: f64,
        wall_y: f64,
        side: HitSide,
        brightness: f64,
        tile_hash: u32,
    ) -> Color {
        self(tile, wall_x, wall_y, side, brightness, tile_hash)
    }
}

/// Simple integer hash for tile coordinates — handy for per-tile variation.
pub fn tile_hash(x: i32, y: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(374_761_393);
    h = h.wrapping_add((y as u32).wrapping_mul(668_265_263));
    h ^= h >> 13;
    h = h.wrapping_mul(1_274_126_177);
    h ^= h >> 16;
    h
}

/// Wall height scale factor controlling how tall walls appear relative to distance.
pub const WALL_HEIGHT_SCALE: f64 = 0.5;

/// Render wall columns into the framebuffer using the supplied texturer.
///
/// `TILE_VOID` columns are left as-is (no wall drawn).
pub fn render_walls(
    fb: &mut Framebuffer,
    rays: &[Option<RayHit>],
    texturer: &dyn WallTexturer,
    max_depth: f64,
) {
    let fb_height = fb.height() as f64;

    for (col, ray) in rays.iter().enumerate() {
        let Some(hit) = ray else { continue };

        if hit.tile == TILE_VOID {
            continue;
        }

        let distance = hit.distance.max(0.001);
        let wall_height = (fb_height / distance * WALL_HEIGHT_SCALE).min(fb_height);
        let wall_top = ((fb_height - wall_height) / 2.0).max(0.0);

        let brightness = (1.0 - distance / max_depth).max(0.0);
        let th = tile_hash(hit.map_x, hit.map_y);

        let y_start = wall_top as usize;
        let y_end = ((wall_top + wall_height) as usize).min(fb.height());

        for y in y_start..y_end {
            let wall_y = if wall_height > 0.0 {
                (y as f64 - wall_top) / wall_height
            } else {
                0.5
            };
            let color =
                texturer.sample_wall(hit.tile, hit.wall_x, wall_y, hit.side, brightness, th);
            fb.set_pixel(col, y, color);
        }
    }
}

/// Render wall columns into the framebuffer, consulting a [`HeightMap`]
/// for per-tile floor / ceiling heights and a [`Camera`] for eye height.
///
/// This generalizes [`render_walls`]: feeding a [`crate::FlatHeightMap`] and
/// a camera with `z = 0.5` produces pixel-identical output to the legacy
/// `render_walls`, including at close range where the projected wall
/// overflows the framebuffer. Diverge from those defaults to draw stepped
/// walls — low fences, tall towers, sunken trenches — without any other
/// renderer change.
///
/// # Projection
///
/// For every ray hit at tile `(map_x, map_y)`:
///
/// ```text
/// floor_h = heights.floor_height(map_x, map_y)
/// ceil_h  = heights.ceiling_height(map_x, map_y)
/// px_per_unit  = fb_height / distance * WALL_HEIGHT_SCALE
/// horizon      = fb_height / 2
/// y_top        = horizon - (ceil_h  - camera.z) * px_per_unit
/// y_bottom     = horizon + (camera.z - floor_h) * px_per_unit
/// ```
///
/// `y_top` / `y_bottom` are then clamped to `[0, fb_height]`, and the
/// texture coordinate `wall_y` is stretched across the **visible** range.
/// This matches the legacy `render_walls` semantics — when a wall is closer
/// than half a tile, the full wall texture is stretched over the whole
/// column rather than being cropped. Callers that want geometry-exact
/// unclamped texturing can derive it from the math above.
///
/// `TILE_VOID` columns are skipped, matching [`render_walls`].
pub fn render_walls_with_heights(
    fb: &mut Framebuffer,
    rays: &[Option<RayHit>],
    texturer: &dyn WallTexturer,
    heights: &dyn HeightMap,
    camera: &Camera,
    max_depth: f64,
) {
    let fb_height = fb.height();
    let fb_h_f = fb_height as f64;
    let horizon = fb_h_f / 2.0;

    for (col, ray) in rays.iter().enumerate() {
        let Some(hit) = ray else { continue };

        if hit.tile == TILE_VOID {
            continue;
        }

        let distance = hit.distance.max(0.001);
        let floor_h = heights.floor_height(hit.map_x, hit.map_y);
        let ceil_h = heights.ceiling_height(hit.map_x, hit.map_y);

        let px_per_unit = fb_h_f / distance * WALL_HEIGHT_SCALE;
        let y_top_f = horizon - (ceil_h - camera.z) * px_per_unit;
        let y_bottom_f = horizon + (camera.z - floor_h) * px_per_unit;

        let brightness = (1.0 - distance / max_depth).max(0.0);
        let th = tile_hash(hit.map_x, hit.map_y);

        // Clamp to framebuffer, and stretch the texture coordinate across
        // the visible (clamped) range. This matches legacy `render_walls`
        // semantics so that a `FlatHeightMap` + `z = 0.5` produces
        // pixel-identical output even at close range where the projected
        // wall overflows the column.
        let visible_top_f = y_top_f.max(0.0).min(fb_h_f);
        let visible_bottom_f = y_bottom_f.max(0.0).min(fb_h_f);
        let visible_height = visible_bottom_f - visible_top_f;
        let y_start = visible_top_f as usize;
        let y_end = visible_bottom_f as usize;

        for y in y_start..y_end {
            let wall_y = if visible_height > 0.0 {
                (y as f64 - visible_top_f) / visible_height
            } else {
                0.5
            };
            let color =
                texturer.sample_wall(hit.tile, hit.wall_x, wall_y, hit.side, brightness, th);
            fb.set_pixel(col, y, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{FlatHeightMap, TILE_WALL};
    use crate::ray::HitSide;

    fn solid_texturer(
        _tile: TileType,
        _wall_x: f64,
        _wall_y: f64,
        _side: HitSide,
        brightness: f64,
        _th: u32,
    ) -> Color {
        Color::rgb(200, 200, 200).darken(brightness)
    }

    fn fake_hit(distance: f64) -> Option<RayHit> {
        Some(RayHit {
            distance,
            side: HitSide::Vertical,
            map_x: 3,
            map_y: 2,
            wall_x: 0.25,
            tile: TILE_WALL,
        })
    }

    #[test]
    fn flat_heights_match_legacy_render_walls_pixel_perfect() {
        // Pick a mix of distances including near-camera (tall walls) and
        // far-away (short walls), plus None and void-like gaps.
        let fb_w: usize = 40;
        let fb_h: usize = 30;
        let rays: Vec<Option<RayHit>> = (0..fb_w)
            .map(|col| match col {
                0..=4 => None,
                5..=14 => fake_hit(1.0 + (col as f64 - 5.0) * 0.3),
                15..=24 => fake_hit(6.0),
                _ => fake_hit(0.3 + (col as f64 - 25.0) * 0.1),
            })
            .collect();

        let mut fb_old = Framebuffer::new(fb_w, fb_h);
        let mut fb_new = Framebuffer::new(fb_w, fb_h);
        fb_old.clear(Color::default());
        fb_new.clear(Color::default());

        render_walls(&mut fb_old, &rays, &solid_texturer, 16.0);
        let cam = Camera::new(0.0, 0.0, 0.0, 1.0);
        render_walls_with_heights(
            &mut fb_new,
            &rays,
            &solid_texturer,
            &FlatHeightMap,
            &cam,
            16.0,
        );

        for y in 0..fb_h {
            for x in 0..fb_w {
                assert_eq!(
                    fb_old.get_pixel(x, y),
                    fb_new.get_pixel(x, y),
                    "pixel mismatch at ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn flat_heights_empty_scene_matches_legacy() {
        // All rays are None — both renderers should leave the framebuffer
        // untouched (pure clear color).
        let fb_w: usize = 16;
        let fb_h: usize = 12;
        let rays: Vec<Option<RayHit>> = vec![None; fb_w];

        let mut fb_old = Framebuffer::new(fb_w, fb_h);
        let mut fb_new = Framebuffer::new(fb_w, fb_h);
        fb_old.clear(Color::rgb(10, 20, 30));
        fb_new.clear(Color::rgb(10, 20, 30));

        render_walls(&mut fb_old, &rays, &solid_texturer, 16.0);
        let cam = Camera::new(0.0, 0.0, 0.0, 1.0);
        render_walls_with_heights(
            &mut fb_new,
            &rays,
            &solid_texturer,
            &FlatHeightMap,
            &cam,
            16.0,
        );

        for y in 0..fb_h {
            for x in 0..fb_w {
                assert_eq!(fb_old.get_pixel(x, y), fb_new.get_pixel(x, y));
            }
        }
    }

    struct TallCeiling;
    impl HeightMap for TallCeiling {
        fn ceiling_height(&self, _x: i32, _y: i32) -> f64 {
            2.0
        }
    }

    #[test]
    fn taller_ceiling_extends_wall_upward() {
        // With a 2.0-high ceiling (vs flat 1.0), the top of the wall should
        // reach noticeably higher than under FlatHeightMap at the same hit.
        let fb_w: usize = 8;
        let fb_h: usize = 60;
        let rays: Vec<Option<RayHit>> = (0..fb_w).map(|_| fake_hit(2.0)).collect();

        let cam = Camera::with_z(0.0, 0.0, 0.5, 0.0, 1.0);

        let mut fb_flat = Framebuffer::new(fb_w, fb_h);
        let mut fb_tall = Framebuffer::new(fb_w, fb_h);
        fb_flat.clear(Color::default());
        fb_tall.clear(Color::default());

        render_walls_with_heights(
            &mut fb_flat,
            &rays,
            &solid_texturer,
            &FlatHeightMap,
            &cam,
            16.0,
        );
        render_walls_with_heights(
            &mut fb_tall,
            &rays,
            &solid_texturer,
            &TallCeiling,
            &cam,
            16.0,
        );

        // Count non-background pixels in column 0 for each framebuffer.
        let col = 0;
        let count_flat = (0..fb_h)
            .filter(|&y| fb_flat.get_pixel(col, y) != Color::default())
            .count();
        let count_tall = (0..fb_h)
            .filter(|&y| fb_tall.get_pixel(col, y) != Color::default())
            .count();

        assert!(
            count_tall > count_flat,
            "tall ceiling should paint more rows than flat (flat={count_flat}, tall={count_tall})"
        );

        // Also: the topmost painted row in the tall case should be above
        // (numerically smaller y) the topmost painted row in the flat case.
        let top_flat = (0..fb_h)
            .find(|&y| fb_flat.get_pixel(col, y) != Color::default())
            .unwrap();
        let top_tall = (0..fb_h)
            .find(|&y| fb_tall.get_pixel(col, y) != Color::default())
            .unwrap();
        assert!(
            top_tall < top_flat,
            "tall ceiling should start higher (top_flat={top_flat}, top_tall={top_tall})"
        );
    }
}
