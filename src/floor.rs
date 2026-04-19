//! Floor and ceiling renderer with corner-interpolated heights.
//!
//! # Algorithm
//!
//! For every screen column the renderer walks the 2D grid with DDA (matching
//! [`crate::ray::cast_ray`]), stopping at either the wall hit distance or
//! the `max_depth` argument of [`render_floor_ceiling`]. For each walked
//! cell it fetches the [`crate::CornerHeights`] and maps that segment of
//! the ray to screen y ranges for both floor and ceiling.
//!
//! Inside a cell the floor surface is treated as bilinear across the four
//! corners, and the ray's intersection with that surface is approximated
//! by linearly interpolating the bilinear sample along the ray path:
//!
//! ```text
//! fh_enter = bilinear(corners.floor, u_enter, v_enter)
//! fh_exit  = bilinear(corners.floor, u_exit , v_exit )
//! fh(d) ≈ fh_enter + (fh_exit - fh_enter) * (d - d_enter) / (d_exit - d_enter)
//! ```
//!
//! The exact intersection of a ray with the double-triangle bilinear
//! surface is quadratic in `d`; the linear approximation across the
//! segment is exact at the cell endpoints and visually indistinguishable
//! in between for the cell-sized segments termray walks. The projection
//! from world z to screen y:
//!
//! ```text
//! y = center_y + (camera.z - fh(d)) * focal_y / d
//! focal_y = fb_height / 2
//! ```
//!
//! (termray's raycaster implicitly uses `vertical_fov ≈ 90°`, so
//! `focal_y = fb_height / 2` is baked in — same convention as the
//! pre-v0.3 implementation.)
//!
//! The two endpoints `d = d_enter` and `d = d_exit` map to `y_enter` and
//! `y_exit`. The rasterizer fills the screen-y interval between them,
//! clips to the wall bounds, and for each y inverts the linear `fh(d)`
//! approximation to recover `d`, `world_x`, `world_y`, and hands those
//! to the [`FloorTexturer`].
//!
//! Ceilings are symmetric — the same math with floor replaced by ceiling
//! and the screen-y sign flipped.

use crate::camera::{Camera, projection_center_y};
use crate::framebuffer::{Color, Framebuffer};
use crate::map::{HeightMap, TILE_VOID};
use crate::ray::RayHit;
use crate::renderer::WALL_HEIGHT_SCALE;

/// Pluggable floor/ceiling texture source.
///
/// Called per pixel with world-space coordinates of the projected floor/ceiling point.
/// Applications decide the visual style (grid, solid, gradient, etc.).
pub trait FloorTexturer {
    fn sample_floor(&self, world_x: f64, world_y: f64, brightness: f64) -> Color;
    fn sample_ceiling(&self, world_x: f64, world_y: f64, brightness: f64) -> Color;
}

/// Render floor and ceiling with corner-interpolated heights via per-column
/// DDA.
///
/// Columns whose ray hit `TILE_VOID` are skipped entirely (background shows through).
///
/// The `heights` argument controls the floor / ceiling surfaces. Pass
/// [`crate::FlatHeightMap`] to reproduce the flat-world look of pre-v0.3
/// termray; pass a non-trivial `HeightMap` to render true corner-interpolated
/// slopes. See the module-level docs for the projection math.
pub fn render_floor_ceiling(
    fb: &mut Framebuffer,
    rays: &[Option<RayHit>],
    texturer: &dyn FloorTexturer,
    heights: &dyn HeightMap,
    camera: &Camera,
    max_depth: f64,
) {
    let fb_width = fb.width();
    let fb_height = fb.height();
    let fb_h_f = fb_height as f64;
    let center_y = projection_center_y(fb_width, fb_height, camera);
    // Vertical focal length for the floor/ceiling projection. Matches the
    // legacy "horizon / row_dist_from_horizon" convention (vertical_fov ≈ 90°).
    let focal_y = fb_h_f / 2.0;

    let dir_x = camera.angle.cos();
    let dir_y = camera.angle.sin();
    let plane_x = -(camera.fov / 2.0).tan() * dir_y;
    let plane_y = (camera.fov / 2.0).tan() * dir_x;

    // Per-column wall bounds: floor/ceiling is drawn outside the wall only.
    // VOID columns cover the entire column to suppress floor/ceiling as well.
    // The wall bounds use the same corner-aware projection as `render_walls`,
    // so the floor/ceiling fills right up to the slanted wall edge.
    let wall_bounds: Vec<(usize, usize, bool)> = rays
        .iter()
        .map(|ray| wall_bounds_for(ray, heights, camera, center_y, fb_h_f, fb_height))
        .collect();

    for (col, ray) in rays.iter().enumerate() {
        let (wall_top, wall_bottom, void) = wall_bounds[col];
        if void {
            continue;
        }

        let camera_frac = (col as f64 / fb_width as f64) * 2.0 - 1.0;
        let hdir_x = dir_x + plane_x * camera_frac;
        let hdir_y = dir_y + plane_y * camera_frac;

        // Hard per-column distance cap: the wall hit distance for columns
        // where a wall exists, else max_depth.
        let d_cap = match ray {
            Some(hit) if hit.tile != TILE_VOID => hit.distance.max(0.001).min(max_depth),
            _ => max_depth,
        };

        // Walk cells along the ray with DDA, tracking entry/exit perpendicular
        // distances for each cell.
        walk_column_floor_ceiling(
            fb,
            texturer,
            heights,
            camera,
            col,
            hdir_x,
            hdir_y,
            d_cap,
            center_y,
            focal_y,
            fb_h_f,
            wall_top,
            wall_bottom,
            fb_height,
        );
    }
}

fn wall_bounds_for(
    ray: &Option<RayHit>,
    heights: &dyn HeightMap,
    camera: &Camera,
    center_y: f64,
    fb_h_f: f64,
    fb_height: usize,
) -> (usize, usize, bool) {
    use crate::renderer::face_edge_heights;

    let Some(hit) = ray else {
        // No wall: floor/ceiling fills the whole column. Use the projection
        // center so both halves have somewhere to end.
        return (center_y as usize, center_y as usize, false);
    };
    if hit.tile == TILE_VOID {
        return (0, fb_height, true);
    }

    let distance = hit.distance.max(0.001);
    let ch = heights.cell_heights(hit.map_x, hit.map_y);
    let (fh0, fh1) = face_edge_heights(&ch.floor, hit.face);
    let (ch0, ch1) = face_edge_heights(&ch.ceil, hit.face);
    let wx = hit.wall_x.clamp(0.0, 1.0);
    let floor_h = fh0 * (1.0 - wx) + fh1 * wx;
    let ceil_h = ch0 * (1.0 - wx) + ch1 * wx;

    let px_per_unit = fb_h_f / distance * WALL_HEIGHT_SCALE;
    let y_top_f = center_y - (ceil_h - camera.z) * px_per_unit;
    let y_bottom_f = center_y + (camera.z - floor_h) * px_per_unit;
    let top = y_top_f.clamp(0.0, fb_h_f);
    let bottom = y_bottom_f.clamp(0.0, fb_h_f);
    (top as usize, bottom as usize, false)
}

#[allow(clippy::too_many_arguments)]
fn walk_column_floor_ceiling(
    fb: &mut Framebuffer,
    texturer: &dyn FloorTexturer,
    heights: &dyn HeightMap,
    camera: &Camera,
    col: usize,
    hdir_x: f64,
    hdir_y: f64,
    d_cap: f64,
    center_y: f64,
    focal_y: f64,
    fb_h_f: f64,
    wall_top: usize,
    wall_bottom: usize,
    fb_height: usize,
) {
    // Standard DDA setup tracking perpendicular distance to cell-boundary
    // crossings, matching `ray::cast_ray`.
    let mut map_x = camera.x.floor() as i32;
    let mut map_y = camera.y.floor() as i32;

    let delta_x = if hdir_x == 0.0 {
        f64::MAX
    } else {
        (1.0 / hdir_x).abs()
    };
    let delta_y = if hdir_y == 0.0 {
        f64::MAX
    } else {
        (1.0 / hdir_y).abs()
    };
    let (step_x, mut side_x) = if hdir_x < 0.0 {
        (-1_i32, (camera.x - map_x as f64) * delta_x)
    } else {
        (1_i32, (map_x as f64 + 1.0 - camera.x) * delta_x)
    };
    let (step_y, mut side_y) = if hdir_y < 0.0 {
        (-1_i32, (camera.y - map_y as f64) * delta_y)
    } else {
        (1_i32, (map_y as f64 + 1.0 - camera.y) * delta_y)
    };

    let mut d_enter = 0.0_f64;
    let mut safety = 0usize;
    loop {
        safety += 1;
        if safety > 4096 {
            break; // pathological loop guard
        }
        let d_exit = side_x.min(side_y).min(d_cap);
        if d_exit <= d_enter {
            // Shouldn't happen, but avoid infinite loops / degenerate fills.
            if d_exit >= d_cap {
                break;
            }
        }
        // Process the segment [d_enter, d_exit] inside cell (map_x, map_y).
        paint_cell_segment(
            fb,
            texturer,
            heights,
            camera,
            col,
            hdir_x,
            hdir_y,
            d_enter,
            d_exit,
            map_x,
            map_y,
            center_y,
            focal_y,
            fb_h_f,
            wall_top,
            wall_bottom,
            fb_height,
        );

        if d_exit >= d_cap {
            break;
        }

        // Advance to next cell.
        if side_x < side_y {
            d_enter = side_x;
            side_x += delta_x;
            map_x += step_x;
        } else {
            d_enter = side_y;
            side_y += delta_y;
            map_y += step_y;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_cell_segment(
    fb: &mut Framebuffer,
    texturer: &dyn FloorTexturer,
    heights: &dyn HeightMap,
    camera: &Camera,
    col: usize,
    hdir_x: f64,
    hdir_y: f64,
    d_enter: f64,
    d_exit: f64,
    cx: i32,
    cy: i32,
    center_y: f64,
    focal_y: f64,
    fb_h_f: f64,
    wall_top: usize,
    wall_bottom: usize,
    fb_height: usize,
) {
    if d_exit - d_enter < 1e-9 {
        return;
    }
    let ch = heights.cell_heights(cx, cy);

    // Local (u, v) in [0,1] of the entry/exit points inside this cell.
    let u_at = |d: f64| camera.x + hdir_x * d - cx as f64;
    let v_at = |d: f64| camera.y + hdir_y * d - cy as f64;
    // Entry/exit sample in exact cell-local coords (tolerate tiny overshoot
    // due to floating point — `sample_floor` extrapolates linearly).
    let (u_e, v_e) = (u_at(d_enter), v_at(d_enter));
    let (u_x, v_x) = (u_at(d_exit), v_at(d_exit));

    // --- Floor layer ---
    let fh_e = ch.sample_floor(u_e.clamp(0.0, 1.0), v_e.clamp(0.0, 1.0));
    let fh_x = ch.sample_floor(u_x.clamp(0.0, 1.0), v_x.clamp(0.0, 1.0));
    paint_layer(
        fb,
        col,
        d_enter,
        d_exit,
        fh_e,
        fh_x,
        camera.z,
        center_y,
        focal_y,
        fb_h_f,
        fb_height,
        wall_top,
        wall_bottom,
        hdir_x,
        hdir_y,
        camera.x,
        camera.y,
        Layer::Floor,
        texturer,
    );

    // --- Ceiling layer ---
    let gh_e = ch.sample_ceil(u_e.clamp(0.0, 1.0), v_e.clamp(0.0, 1.0));
    let gh_x = ch.sample_ceil(u_x.clamp(0.0, 1.0), v_x.clamp(0.0, 1.0));
    paint_layer(
        fb,
        col,
        d_enter,
        d_exit,
        gh_e,
        gh_x,
        camera.z,
        center_y,
        focal_y,
        fb_h_f,
        fb_height,
        wall_top,
        wall_bottom,
        hdir_x,
        hdir_y,
        camera.x,
        camera.y,
        Layer::Ceiling,
        texturer,
    );
}

#[derive(Clone, Copy)]
enum Layer {
    Floor,
    Ceiling,
}

/// Fill the screen-y range produced by projecting a linearly-interpolated
/// floor or ceiling surface over the ray segment `[d_enter, d_exit]`.
///
/// Math (floor case; ceiling is symmetric with `h_*` being the ceiling
/// heights):
///
/// ```text
/// h(d) = a + b*d,   where
///   a = h_enter - b * d_enter
///   b = (h_exit - h_enter) / (d_exit - d_enter)
///
/// y(d) = center_y + focal_y * (camera.z - h(d)) / d
///       = center_y + focal_y * ((camera.z - a) / d - b)
///
/// Inverse (for stepping per-pixel across the span):
///   dy  = y - center_y
///   d   = focal_y * (camera.z - a) / (dy + focal_y * b)
/// ```
///
/// For `fh_enter == fh_exit` the surface is flat across the cell segment
/// and the inverse reduces to the standard raycaster formula
/// `d = focal_y * (camera.z - floor_h) / (y - center_y)`.
#[allow(clippy::too_many_arguments)]
fn paint_layer(
    fb: &mut Framebuffer,
    col: usize,
    d_enter: f64,
    d_exit: f64,
    h_enter: f64,
    h_exit: f64,
    cam_z: f64,
    center_y: f64,
    focal_y: f64,
    fb_h_f: f64,
    fb_height: usize,
    wall_top: usize,
    wall_bottom: usize,
    hdir_x: f64,
    hdir_y: f64,
    cam_x: f64,
    cam_y: f64,
    layer: Layer,
    texturer: &dyn FloorTexturer,
) {
    // Compute y at d_enter and d_exit.
    let y_enter = center_y + focal_y * (cam_z - h_enter) / d_enter;
    let y_exit = center_y + focal_y * (cam_z - h_exit) / d_exit;

    // Select which half of the screen this layer occupies.
    let (y_min_f, y_max_f) = if y_enter <= y_exit {
        (y_enter, y_exit)
    } else {
        (y_exit, y_enter)
    };

    // Floor lives below center_y (y > center_y), ceiling above (y < center_y).
    // Clip the span to the correct half first.
    let (span_lo, span_hi) = match layer {
        Layer::Floor => (y_min_f.max(center_y), y_max_f.max(center_y)),
        Layer::Ceiling => (y_min_f.min(center_y), y_max_f.min(center_y)),
    };
    if span_hi <= span_lo {
        return;
    }

    // Further clip to wall bounds and framebuffer extents.
    let (clip_lo, clip_hi) = match layer {
        Layer::Floor => (
            span_lo.max(wall_bottom as f64),
            span_hi.min(fb_h_f).min(fb_height as f64),
        ),
        Layer::Ceiling => (span_lo.max(0.0), span_hi.min(wall_top as f64)),
    };
    if clip_hi <= clip_lo {
        return;
    }

    // Linear parameterization h(d) = a + b*d over [d_enter, d_exit].
    let dd = d_exit - d_enter;
    let b = if dd > 1e-12 {
        (h_exit - h_enter) / dd
    } else {
        0.0
    };
    let a = h_enter - b * d_enter;

    let y_start = clip_lo.floor() as isize;
    let y_end = clip_hi.ceil() as isize;
    for y_i in y_start..y_end {
        if y_i < 0 || (y_i as usize) >= fb_height {
            continue;
        }
        let y = y_i as f64 + 0.5;
        match layer {
            Layer::Floor => {
                if y <= center_y {
                    continue;
                }
                if (y_i as usize) < wall_bottom {
                    continue;
                }
            }
            Layer::Ceiling => {
                if y >= center_y {
                    continue;
                }
                if (y_i as usize) >= wall_top {
                    continue;
                }
            }
        }
        // Invert y(d) = center_y + focal_y * ((cam_z - a) / d - b)
        // → d = focal_y * (cam_z - a) / (y - center_y + focal_y * b)
        let denom = y - center_y + focal_y * b;
        if denom.abs() < 1e-9 {
            continue;
        }
        let d = focal_y * (cam_z - a) / denom;
        if !d.is_finite() || d < d_enter - 1e-6 || d > d_exit + 1e-6 {
            continue;
        }
        let d = d.clamp(d_enter.max(1e-9), d_exit);

        let wx = cam_x + hdir_x * d;
        let wy = cam_y + hdir_y * d;
        let brightness = (1.0 / (1.0 + d * 0.15)).clamp(0.08, 1.0);
        let color = match layer {
            Layer::Floor => texturer.sample_floor(wx, wy, brightness),
            Layer::Ceiling => texturer.sample_ceiling(wx, wy, brightness),
        };
        fb.set_pixel(col, y_i as usize, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framebuffer::Framebuffer;
    use crate::map::{CornerHeights, FlatHeightMap, HeightMap};
    use crate::ray::{HitFace, HitSide};

    struct Checker;
    impl FloorTexturer for Checker {
        fn sample_floor(&self, wx: f64, wy: f64, brightness: f64) -> Color {
            let c = ((wx.floor() as i32 + wy.floor() as i32) & 1) as u8;
            Color::rgb(100 + 80 * c, 80, 60).darken(brightness)
        }
        fn sample_ceiling(&self, _wx: f64, _wy: f64, brightness: f64) -> Color {
            Color::rgb(40, 60, 100).darken(brightness)
        }
    }

    #[test]
    fn flat_world_fills_both_halves() {
        let fb_w: usize = 40;
        let fb_h: usize = 40;
        let mut fb = Framebuffer::new(fb_w, fb_h);
        fb.clear(Color::default());

        let cam = Camera::new(5.0, 5.0, 0.3, 70f64.to_radians());
        let rays: Vec<Option<RayHit>> = (0..fb_w).map(|_| None).collect();
        render_floor_ceiling(&mut fb, &rays, &Checker, &FlatHeightMap, &cam, 16.0);

        let upper = (0..fb_h / 2)
            .map(|y| {
                (0..fb_w)
                    .filter(|&x| fb.get_pixel(x, y) != Color::default())
                    .count()
            })
            .sum::<usize>();
        let lower = ((fb_h / 2)..fb_h)
            .map(|y| {
                (0..fb_w)
                    .filter(|&x| fb.get_pixel(x, y) != Color::default())
                    .count()
            })
            .sum::<usize>();
        assert!(upper > 0, "ceiling should have pixels painted");
        assert!(lower > 0, "floor should have pixels painted");
    }

    struct Hill;
    impl HeightMap for Hill {
        fn cell_heights(&self, x: i32, y: i32) -> CornerHeights {
            // Raised hill centered near (5,5) so distant tiles are higher.
            let fh_at = |cx: i32, cy: i32| -> f64 {
                let dx = cx - 5;
                let dy = cy - 5;
                let d2 = (dx * dx + dy * dy) as f64;
                (0.5 - d2 * 0.03).max(-0.2)
            };
            CornerHeights {
                floor: [
                    fh_at(x, y),
                    fh_at(x + 1, y),
                    fh_at(x, y + 1),
                    fh_at(x + 1, y + 1),
                ],
                ceil: [1.0; 4],
            }
        }
    }

    #[test]
    fn sloped_floor_renders_without_panicking() {
        // Smoke test: exercise the corner-interpolated path with a
        // continuous heightmap and a few ray hits.
        let fb_w: usize = 40;
        let fb_h: usize = 40;
        let mut fb = Framebuffer::new(fb_w, fb_h);
        fb.clear(Color::default());

        let cam = Camera::with_z(5.0, 5.0, 0.9, 0.0, 70f64.to_radians());
        let rays: Vec<Option<RayHit>> = (0..fb_w)
            .map(|col| {
                if col % 5 == 0 {
                    Some(RayHit {
                        distance: 6.0,
                        side: HitSide::Vertical,
                        face: HitFace::West,
                        map_x: 9,
                        map_y: 5,
                        wall_x: 0.5,
                        tile: 1,
                    })
                } else {
                    None
                }
            })
            .collect();
        render_floor_ceiling(&mut fb, &rays, &Checker, &Hill, &cam, 12.0);
        let painted = (0..fb_h)
            .flat_map(|y| (0..fb_w).map(move |x| (x, y)))
            .filter(|&(x, y)| fb.get_pixel(x, y) != Color::default())
            .count();
        assert!(painted > 0, "hill scene should paint some floor pixels");
    }
}
