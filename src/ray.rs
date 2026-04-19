use crate::map::{TILE_VOID, TileMap};
use crate::math::Vec2f;

/// Which axis a [`RayHit`] crossed when the DDA ended.
///
/// Coarser than [`HitFace`]: `Vertical` means the ray ended on an
/// x-aligned cell boundary (i.e. hit a West or East face), `Horizontal`
/// means it ended on a y-aligned boundary (North or South face).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitSide {
    Vertical,
    Horizontal,
}

/// Which of the four cell faces a [`RayHit`] struck.
///
/// Named after the compass direction of the face's normal:
///
/// - `West`  — face at `x = map_x`,     normal pointing −x. The ray came from
///   the west (its `step_x > 0`).
/// - `East`  — face at `x = map_x + 1`, normal pointing +x. The ray came from
///   the east (its `step_x < 0`).
/// - `North` — face at `y = map_y`,     normal pointing −y. The ray came from
///   the north (its `step_y > 0`).
/// - `South` — face at `y = map_y + 1`, normal pointing +y. The ray came from
///   the south (its `step_y < 0`).
///
/// The corner-interpolating wall renderer uses this to pick the correct
/// pair of corner heights from [`crate::CornerHeights`] for the hit face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitFace {
    West,
    East,
    North,
    South,
}

#[derive(Debug, Clone)]
pub struct RayHit {
    pub distance: f64,
    pub side: HitSide,
    /// Which of the four cell faces the ray hit. Used by the
    /// corner-interpolating wall renderer to pick the right edge of the
    /// cell's [`crate::CornerHeights`]. See [`HitFace`].
    pub face: HitFace,
    pub map_x: i32,
    pub map_y: i32,
    /// Fractional position along the wall surface (0.0..1.0).
    ///
    /// For `TILE_VOID` this is set to `f64::NAN` as a sentinel — VOID hits are
    /// meant to be consumed without face interpolation, and routing an NaN
    /// through any accidental sampler will surface the mistake as a visible
    /// NaN-poisoned region rather than an off-by-a-fraction texture.
    ///
    /// The direction of increasing `wall_x` across the face follows the
    /// face's internal orientation (see [`HitFace`]):
    ///
    /// - `West`  face: 0.0 at the **NW** corner, 1.0 at the **SW** corner.
    /// - `East`  face: 0.0 at the **NE** corner, 1.0 at the **SE** corner.
    /// - `North` face: 0.0 at the **NW** corner, 1.0 at the **NE** corner.
    /// - `South` face: 0.0 at the **SW** corner, 1.0 at the **SE** corner.
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

            // Identify which of the four faces of this cell the ray crossed.
            // Vertical side + east-going ray (step_x > 0) means the ray hit
            // the cell's West face; same side + west-going ray hits the East
            // face. Same logic for Horizontal + N/S.
            let face = match side {
                HitSide::Vertical => {
                    if step_x > 0 {
                        HitFace::West
                    } else {
                        HitFace::East
                    }
                }
                HitSide::Horizontal => {
                    if step_y > 0 {
                        HitFace::North
                    } else {
                        HitFace::South
                    }
                }
            };

            if tile == TILE_VOID {
                // VOID columns are never sampled for face interpolation — all
                // callers short-circuit on `tile == TILE_VOID` before touching
                // `wall_x`. Use NaN as a sentinel so any bug that quietly
                // threads it through (e.g. a future renderer forgetting the
                // VOID branch) surfaces as an NaN-poisoned calculation rather
                // than a silently-off texture coordinate.
                return Some(RayHit {
                    distance: perp_dist,
                    side,
                    face,
                    map_x,
                    map_y,
                    wall_x: f64::NAN,
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
                face,
                map_x,
                map_y,
                wall_x,
                tile,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{TILE_EMPTY, TileMap};

    /// Tiny GridMap wrapper that reports cell (2, 2) as TILE_VOID so we can
    /// exercise the VOID branch of `cast_ray`. Everything else walkable.
    struct VoidMap;
    impl TileMap for VoidMap {
        fn width(&self) -> usize {
            8
        }
        fn height(&self) -> usize {
            8
        }
        fn get(&self, x: i32, y: i32) -> Option<crate::map::TileType> {
            if !(0..8).contains(&x) || !(0..8).contains(&y) {
                return None;
            }
            if x == 2 && y == 0 {
                Some(TILE_VOID)
            } else {
                Some(TILE_EMPTY)
            }
        }
        fn is_solid(&self, x: i32, y: i32) -> bool {
            !matches!(self.get(x, y), Some(TILE_EMPTY))
        }
    }

    #[test]
    fn void_hit_reports_nan_wall_x() {
        // Ray goes +x from (0.5, 0.5); the VOID cell at (2, 0) is hit after
        // walking across (1, 0). The hit's `wall_x` must be NaN so any
        // accidental face-interpolation consumer produces NaN-poisoned
        // output instead of a plausible-but-wrong texture coordinate.
        let map = VoidMap;
        let hit = cast_ray(&map, Vec2f::new(0.5, 0.5), 0.0, 16.0).expect("should hit VOID");
        assert_eq!(hit.tile, TILE_VOID);
        assert!(
            hit.wall_x.is_nan(),
            "VOID wall_x should be NaN sentinel, got {}",
            hit.wall_x
        );
        // distance must still be finite so callers can do depth-testing.
        assert!(hit.distance.is_finite());
    }
}
