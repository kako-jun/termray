use crate::camera::{Camera, projection_center_y};
use crate::framebuffer::{Color, Framebuffer};
use crate::map::{CORNER_NE, CORNER_NW, CORNER_SE, CORNER_SW, HeightMap, TILE_VOID, TileType};
use crate::ray::{HitFace, HitSide, RayHit};

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

/// Pick the two corner heights of the cell's floor / ceiling that bound
/// the hit face, returning them in `(at_wall_x_0, at_wall_x_1)` order.
///
/// Order matches the convention on [`crate::RayHit::wall_x`]:
///
/// - `West`  → (NW, SW)
/// - `East`  → (NE, SE)
/// - `North` → (NW, NE)
/// - `South` → (SW, SE)
#[inline]
pub(crate) fn face_edge_heights(corners: &[f64; 4], face: HitFace) -> (f64, f64) {
    match face {
        HitFace::West => (corners[CORNER_NW], corners[CORNER_SW]),
        HitFace::East => (corners[CORNER_NE], corners[CORNER_SE]),
        HitFace::North => (corners[CORNER_NW], corners[CORNER_NE]),
        HitFace::South => (corners[CORNER_SW], corners[CORNER_SE]),
    }
}

/// Render wall columns into the framebuffer, consulting a [`HeightMap`]
/// for per-cell corner floor / ceiling heights and a [`Camera`] for eye
/// height and pitch.
///
/// For each ray hit the renderer fetches the [`crate::CornerHeights`] of
/// the hit cell, picks the two corners bounding the hit face (via
/// [`RayHit::face`]), linearly interpolates floor and ceiling across the
/// face using `wall_x`, and projects the resulting top / bottom world
/// heights into screen space.
///
/// Pass [`crate::FlatHeightMap`] to get tile-flat walls, or any other
/// `HeightMap` to get corner-interpolated slopes.
///
/// # Projection
///
/// ```text
/// (fh0, fh1) = floor corners at the hit face in wall_x=0..1 order
/// (ch0, ch1) = ceiling corners at the hit face in wall_x=0..1 order
/// floor_h = lerp(fh0, fh1, wall_x)
/// ceil_h  = lerp(ch0, ch1, wall_x)
/// px_per_unit = fb_height / distance * WALL_HEIGHT_SCALE
/// y_top    = center_y - (ceil_h  - camera.z) * px_per_unit
/// y_bottom = center_y + (camera.z - floor_h) * px_per_unit
/// ```
///
/// `center_y = fb_height / 2 + tan(pitch) * focal_px(fb_width, fov)` —
/// pitch shifts the horizon but otherwise leaves the projection unchanged
/// (see [`Camera`] for the horizon-shift convention).
///
/// `y_top` / `y_bottom` are clamped to `[0, fb_height]` and the texture
/// coordinate `wall_y` is stretched across the visible range — matching
/// pre-v0.3 `render_walls` semantics so a flat camera pressed up against
/// a wall still stretches the full texture across the column rather than
/// cropping it.
///
/// `TILE_VOID` columns are skipped (no wall drawn).
pub fn render_walls(
    fb: &mut Framebuffer,
    rays: &[Option<RayHit>],
    texturer: &dyn WallTexturer,
    heights: &dyn HeightMap,
    camera: &Camera,
    max_depth: f64,
) {
    let fb_width = fb.width();
    let fb_height = fb.height();
    let fb_h_f = fb_height as f64;
    let center_y = projection_center_y(fb_width, fb_height, camera);

    for (col, ray) in rays.iter().enumerate() {
        let Some(hit) = ray else { continue };

        if hit.tile == TILE_VOID {
            continue;
        }

        let distance = hit.distance.max(0.001);

        // Corner-interpolated floor / ceiling at the hit face.
        let ch = heights.cell_heights(hit.map_x, hit.map_y);
        let (fh0, fh1) = face_edge_heights(&ch.floor, hit.face);
        let (ch0, ch1) = face_edge_heights(&ch.ceil, hit.face);
        let wx = hit.wall_x.clamp(0.0, 1.0);
        let floor_h = fh0 * (1.0 - wx) + fh1 * wx;
        let ceil_h = ch0 * (1.0 - wx) + ch1 * wx;

        let px_per_unit = fb_h_f / distance * WALL_HEIGHT_SCALE;
        let y_top_f = center_y - (ceil_h - camera.z) * px_per_unit;
        let y_bottom_f = center_y + (camera.z - floor_h) * px_per_unit;

        let brightness = (1.0 - distance / max_depth).max(0.0);
        let th = tile_hash(hit.map_x, hit.map_y);

        // Clamp to framebuffer, and stretch the texture coordinate across
        // the visible (clamped) range.
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
    use crate::map::{CornerHeights, FlatHeightMap, HeightMap, TILE_WALL};
    use crate::ray::{HitFace, HitSide};

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
            face: HitFace::West,
            map_x: 3,
            map_y: 2,
            wall_x: 0.25,
            tile: TILE_WALL,
        })
    }

    #[test]
    fn flat_heights_paint_nonempty_column() {
        let fb_w: usize = 40;
        let fb_h: usize = 30;
        let rays: Vec<Option<RayHit>> = (0..fb_w).map(|_| fake_hit(3.0)).collect();
        let mut fb = Framebuffer::new(fb_w, fb_h);
        fb.clear(Color::default());

        let cam = Camera::new(0.0, 0.0, 0.0, 70f64.to_radians());
        render_walls(&mut fb, &rays, &solid_texturer, &FlatHeightMap, &cam, 16.0);

        let painted = (0..fb_h)
            .filter(|&y| fb.get_pixel(0, y) != Color::default())
            .count();
        assert!(painted > 0, "flat map should paint a wall strip");
    }

    #[test]
    fn void_columns_are_skipped() {
        let fb_w: usize = 8;
        let fb_h: usize = 12;
        let rays: Vec<Option<RayHit>> = (0..fb_w)
            .map(|_| {
                Some(RayHit {
                    distance: 2.0,
                    side: HitSide::Vertical,
                    face: HitFace::West,
                    map_x: 0,
                    map_y: 0,
                    wall_x: 0.5,
                    tile: TILE_VOID,
                })
            })
            .collect();

        let mut fb = Framebuffer::new(fb_w, fb_h);
        fb.clear(Color::default());
        let cam = Camera::new(0.0, 0.0, 0.0, 70f64.to_radians());
        render_walls(&mut fb, &rays, &solid_texturer, &FlatHeightMap, &cam, 16.0);

        for y in 0..fb_h {
            for x in 0..fb_w {
                assert_eq!(fb.get_pixel(x, y), Color::default());
            }
        }
    }

    struct TallCeiling;
    impl HeightMap for TallCeiling {
        fn cell_heights(&self, _x: i32, _y: i32) -> CornerHeights {
            CornerHeights {
                floor: [0.0; 4],
                ceil: [2.0; 4],
            }
        }
    }

    #[test]
    fn taller_ceiling_extends_wall_upward() {
        let fb_w: usize = 8;
        let fb_h: usize = 60;
        let rays: Vec<Option<RayHit>> = (0..fb_w).map(|_| fake_hit(2.0)).collect();

        let cam = Camera::with_z(0.0, 0.0, 0.5, 0.0, 70f64.to_radians());

        let mut fb_flat = Framebuffer::new(fb_w, fb_h);
        let mut fb_tall = Framebuffer::new(fb_w, fb_h);
        fb_flat.clear(Color::default());
        fb_tall.clear(Color::default());

        render_walls(
            &mut fb_flat,
            &rays,
            &solid_texturer,
            &FlatHeightMap,
            &cam,
            16.0,
        );
        render_walls(
            &mut fb_tall,
            &rays,
            &solid_texturer,
            &TallCeiling,
            &cam,
            16.0,
        );

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

    struct SlopedCeiling;
    impl HeightMap for SlopedCeiling {
        fn cell_heights(&self, _x: i32, _y: i32) -> CornerHeights {
            // West edge (NW,SW) at 1.0, East edge (NE,SE) at 2.0.
            CornerHeights {
                floor: [0.0; 4],
                ceil: [1.0, 2.0, 1.0, 2.0],
            }
        }
    }

    #[test]
    fn sloped_face_interpolates_linearly_along_wall_x() {
        // Ray hitting the North face (spans NW→NE in wall_x 0→1) of a cell
        // whose ceiling slopes from 1.0 at NW to 2.0 at NE — wall top at
        // wall_x=1.0 should sit strictly higher on screen than at wall_x=0.0.
        let fb_h: usize = 60;
        let fb_w: usize = 4;
        let distance = 2.0;

        let cam = Camera::with_z(0.0, 0.0, 0.5, 0.0, 70f64.to_radians());
        let mut tops: Vec<usize> = Vec::new();
        for wx in &[0.0_f64, 0.5, 1.0] {
            let rays: Vec<Option<RayHit>> = (0..fb_w)
                .map(|_| {
                    Some(RayHit {
                        distance,
                        side: HitSide::Horizontal,
                        face: HitFace::North,
                        map_x: 0,
                        map_y: 0,
                        wall_x: *wx,
                        tile: TILE_WALL,
                    })
                })
                .collect();
            let mut fb = Framebuffer::new(fb_w, fb_h);
            fb.clear(Color::default());
            render_walls(&mut fb, &rays, &solid_texturer, &SlopedCeiling, &cam, 16.0);
            let top = (0..fb_h)
                .find(|&y| fb.get_pixel(0, y) != Color::default())
                .unwrap_or(fb_h);
            tops.push(top);
        }
        // Higher ceiling (wall_x=1.0) → smaller y for the top pixel.
        assert!(
            tops[0] > tops[1] && tops[1] > tops[2],
            "tops should strictly decrease with wall_x, got {:?}",
            tops
        );
    }

    #[test]
    fn pitch_shifts_wall_strip_vertically() {
        // Framebuffer width drives `focal_px = (fb_w / 2) / tan(fov / 2)`
        // and therefore the magnitude of the pitch horizon shift in pixels
        // (`tan(pitch) * focal_px`). A too-narrow framebuffer rounds the
        // shift below 1 pixel and the test observes no movement.
        let fb_w: usize = 120;
        let fb_h: usize = 80;
        let rays: Vec<Option<RayHit>> = (0..fb_w).map(|_| fake_hit(3.0)).collect();

        let mut cam = Camera::new(0.0, 0.0, 0.0, 70f64.to_radians());

        let mut fb0 = Framebuffer::new(fb_w, fb_h);
        fb0.clear(Color::default());
        render_walls(&mut fb0, &rays, &solid_texturer, &FlatHeightMap, &cam, 16.0);
        let top0 = (0..fb_h)
            .find(|&y| fb0.get_pixel(0, y) != Color::default())
            .unwrap();

        cam.set_pitch(0.2); // look up → horizon shifts *down* → wall strip shifts down too
        let mut fb_up = Framebuffer::new(fb_w, fb_h);
        fb_up.clear(Color::default());
        render_walls(
            &mut fb_up,
            &rays,
            &solid_texturer,
            &FlatHeightMap,
            &cam,
            16.0,
        );
        let top_up = (0..fb_h)
            .find(|&y| fb_up.get_pixel(0, y) != Color::default())
            .unwrap();

        assert!(
            top_up > top0,
            "pitch up should move wall strip downward (top0={top0}, top_up={top_up})"
        );
    }
}
