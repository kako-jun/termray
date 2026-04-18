use crate::framebuffer::{Color, Framebuffer};
use crate::map::{TileType, TILE_VOID};
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
