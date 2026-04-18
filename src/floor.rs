use crate::camera::Camera;
use crate::framebuffer::{Color, Framebuffer};
use crate::map::TILE_VOID;
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

/// Render floor and ceiling with perspective-correct sampling.
///
/// Columns whose ray hit `TILE_VOID` are skipped entirely (background shows through).
pub fn render_floor_ceiling(
    fb: &mut Framebuffer,
    rays: &[Option<RayHit>],
    texturer: &dyn FloorTexturer,
    camera: &Camera,
) {
    let fb_width = fb.width();
    let fb_height = fb.height();
    let fb_h_f = fb_height as f64;
    let horizon = fb_h_f / 2.0;

    let dir_x = camera.angle.cos();
    let dir_y = camera.angle.sin();
    let plane_x = -(camera.fov / 2.0).tan() * dir_y;
    let plane_y = (camera.fov / 2.0).tan() * dir_x;

    // Per-column wall bounds: floor/ceiling is drawn outside the wall only.
    // VOID columns cover the entire column to suppress floor/ceiling as well.
    let wall_bounds: Vec<(usize, usize)> = rays
        .iter()
        .map(|ray| {
            if let Some(hit) = ray {
                if hit.tile == TILE_VOID {
                    (0, fb_height)
                } else {
                    let distance = hit.distance.max(0.001);
                    let wall_height = (fb_h_f / distance * WALL_HEIGHT_SCALE).min(fb_h_f);
                    let top = ((fb_h_f - wall_height) / 2.0).max(0.0);
                    (top as usize, ((top + wall_height) as usize).min(fb_height))
                }
            } else {
                (fb_height / 2, fb_height / 2)
            }
        })
        .collect();

    for y in 0..fb_height {
        let row_dist_from_horizon = (y as f64 - horizon).abs();
        if row_dist_from_horizon < 0.5 {
            continue;
        }

        let is_floor = y as f64 > horizon;
        let row_distance = horizon / row_dist_from_horizon;
        let brightness = (1.0 / (1.0 + row_distance * 0.15)).clamp(0.08, 1.0);

        for (col, &(wall_top, wall_bottom)) in wall_bounds.iter().enumerate() {
            if is_floor && y < wall_bottom {
                continue;
            }
            if !is_floor && y >= wall_top {
                continue;
            }

            let camera_frac = (col as f64 / fb_width as f64) * 2.0 - 1.0;
            let floor_x = camera.x + (dir_x + plane_x * camera_frac) * row_distance;
            let floor_y = camera.y + (dir_y + plane_y * camera_frac) * row_distance;

            let color = if is_floor {
                texturer.sample_floor(floor_x, floor_y, brightness)
            } else {
                texturer.sample_ceiling(floor_x, floor_y, brightness)
            };
            fb.set_pixel(col, y, color);
        }
    }
}
