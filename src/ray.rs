use crate::map::{TILE_VOID, TileMap};
use crate::math::Vec2f;

#[derive(Debug, Clone, Copy)]
pub enum HitSide {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone)]
pub struct RayHit {
    pub distance: f64,
    pub side: HitSide,
    pub map_x: i32,
    pub map_y: i32,
    /// Fractional position along the wall surface (0.0..1.0). For `TILE_VOID`, unspecified (0.0).
    pub wall_x: f64,
    pub tile: u8,
}

/// Cast a single ray using the DDA algorithm. Returns the first solid tile hit,
/// with perpendicular distance (fisheye-free) and surface coordinate.
///
/// `TILE_VOID` tiles are reported as hits with `tile == TILE_VOID` so callers can
/// distinguish map-edge voids from max-depth misses and skip wall/floor rendering
/// for that column.
pub fn cast_ray(map: &dyn TileMap, origin: Vec2f, angle: f64, max_depth: f64) -> Option<RayHit> {
    let ray_dir_x = angle.cos();
    let ray_dir_y = angle.sin();

    let mut map_x = origin.x.floor() as i32;
    let mut map_y = origin.y.floor() as i32;

    let delta_dist_x = if ray_dir_x == 0.0 {
        f64::MAX
    } else {
        (1.0 / ray_dir_x).abs()
    };
    let delta_dist_y = if ray_dir_y == 0.0 {
        f64::MAX
    } else {
        (1.0 / ray_dir_y).abs()
    };

    let (step_x, mut side_dist_x) = if ray_dir_x < 0.0 {
        (-1_i32, (origin.x - map_x as f64) * delta_dist_x)
    } else {
        (1_i32, (map_x as f64 + 1.0 - origin.x) * delta_dist_x)
    };

    let (step_y, mut side_dist_y) = if ray_dir_y < 0.0 {
        (-1_i32, (origin.y - map_y as f64) * delta_dist_y)
    } else {
        (1_i32, (map_y as f64 + 1.0 - origin.y) * delta_dist_y)
    };

    let mut side;

    loop {
        if side_dist_x < side_dist_y {
            side_dist_x += delta_dist_x;
            map_x += step_x;
            side = HitSide::Vertical;
        } else {
            side_dist_y += delta_dist_y;
            map_y += step_y;
            side = HitSide::Horizontal;
        }

        if map_x < 0 || map_y < 0 || map_x >= map.width() as i32 || map_y >= map.height() as i32 {
            return None;
        }

        if map.is_solid(map_x, map_y) {
            let tile = map.get(map_x, map_y).unwrap_or(1);

            let perp_dist = match side {
                HitSide::Vertical => {
                    (map_x as f64 - origin.x + (1.0 - step_x as f64) / 2.0) / ray_dir_x
                }
                HitSide::Horizontal => {
                    (map_y as f64 - origin.y + (1.0 - step_y as f64) / 2.0) / ray_dir_y
                }
            };

            if tile == TILE_VOID {
                return Some(RayHit {
                    distance: perp_dist,
                    side,
                    map_x,
                    map_y,
                    wall_x: 0.0,
                    tile: TILE_VOID,
                });
            }

            if perp_dist > max_depth {
                return None;
            }

            let wall_x = match side {
                HitSide::Vertical => {
                    let hit = origin.y + perp_dist * ray_dir_y;
                    hit - hit.floor()
                }
                HitSide::Horizontal => {
                    let hit = origin.x + perp_dist * ray_dir_x;
                    hit - hit.floor()
                }
            };

            return Some(RayHit {
                distance: perp_dist,
                side,
                map_x,
                map_y,
                wall_x,
                tile,
            });
        }
    }
}
