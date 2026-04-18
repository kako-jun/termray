pub type TileType = u8;

/// Walkable space.
pub const TILE_EMPTY: TileType = 0;
/// Solid, textured wall.
pub const TILE_WALL: TileType = 1;
/// Solid but invisible — used to express "outside the map" regions
/// that the player cannot enter and that produce no wall rendering.
pub const TILE_VOID: TileType = 2;

/// Trait providing tile data to the raycaster.
///
/// Implementations decide what each tile ID means; termray only cares about
/// `TILE_EMPTY`, `TILE_WALL`, and `TILE_VOID`. All other IDs are passed through
/// to [`crate::WallTexturer`].
///
/// # `is_solid` contract
///
/// `is_solid` is the sole authority for "does a ray stop here?". The raycaster
/// ([`crate::ray::cast_ray`]) calls it on every cell it steps onto and stops at
/// the first `true`. The same method is the recommended predicate for
/// movement / collision in your game loop, so rays and the player agree on
/// what counts as a wall.
///
/// - Out-of-bounds coordinates **must** return `true`. The raycaster bails out
///   before ever calling `is_solid` for out-of-bounds cells, but movement
///   collision checks commonly do.
/// - `TILE_VOID` cells **must** be solid (they are impassable by definition).
///   The renderer distinguishes VOID from other solids via [`crate::TILE_VOID`]
///   after the fact.
pub trait TileMap {
    fn width(&self) -> usize;
    fn height(&self) -> usize;
    fn get(&self, x: i32, y: i32) -> Option<TileType>;
    fn is_solid(&self, x: i32, y: i32) -> bool;
}

/// Dense grid map backed by a `Vec<TileType>`. Initial tiles are `TILE_WALL`.
pub struct GridMap {
    width: usize,
    height: usize,
    tiles: Vec<TileType>,
}

impl GridMap {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            tiles: vec![TILE_WALL; width * height],
        }
    }

    pub fn set(&mut self, x: usize, y: usize, tile: TileType) {
        if x < self.width && y < self.height {
            self.tiles[y * self.width + x] = tile;
        }
    }
}

impl TileMap for GridMap {
    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn get(&self, x: i32, y: i32) -> Option<TileType> {
        if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height {
            Some(self.tiles[y as usize * self.width + x as usize])
        } else {
            None
        }
    }

    fn is_solid(&self, x: i32, y: i32) -> bool {
        !matches!(self.get(x, y), Some(TILE_EMPTY))
    }
}

/// World-space heights for floor and ceiling surfaces per tile.
///
/// Both methods have a default flat implementation (0.0 / 1.0) so callers can
/// start with [`FlatHeightMap`] and opt into variation only where needed.
///
/// # Coordinate contract
///
/// Coordinates can be out-of-bounds; implementations should return sane
/// defaults (typically the same values used inside the map) rather than
/// panic. `termray`'s renderer calls these methods for every rendered wall
/// column, using the `map_x` / `map_y` of the ray hit.
///
/// # Orthogonal to [`TileMap`]
///
/// `HeightMap` is intentionally a separate trait from [`TileMap`] so an
/// application can mix and match: the same type may implement both, or you
/// may pair a `GridMap` with a custom `HeightMap` to keep solidity and
/// surface heights in different data structures.
///
/// # Invariants
///
/// Implementations should guarantee `ceiling_height(x, y) >= floor_height(x, y)`
/// for every coordinate. Violations don't panic — the renderer silently
/// skips columns where the projected wall inverts — but the resulting
/// picture is undefined.
pub trait HeightMap {
    /// World-space height of the floor surface at tile `(x, y)`.
    /// Default: `0.0`.
    fn floor_height(&self, _x: i32, _y: i32) -> f64 {
        0.0
    }

    /// World-space height of the ceiling surface at tile `(x, y)`.
    /// Default: `1.0`.
    fn ceiling_height(&self, _x: i32, _y: i32) -> f64 {
        1.0
    }
}

/// Zero-sized [`HeightMap`] implementing a fully flat world
/// (floor=0.0, ceiling=1.0).
///
/// Use this when you don't need per-tile height variation. The legacy
/// [`crate::render_walls`] function implicitly behaves as if a
/// `FlatHeightMap` were active.
pub struct FlatHeightMap;

impl HeightMap for FlatHeightMap {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gridmap_defaults_to_walls() {
        let m = GridMap::new(4, 4);
        assert_eq!(m.get(0, 0), Some(TILE_WALL));
        assert!(m.is_solid(0, 0));
    }

    #[test]
    fn gridmap_empty_is_walkable() {
        let mut m = GridMap::new(4, 4);
        m.set(1, 1, TILE_EMPTY);
        assert_eq!(m.get(1, 1), Some(TILE_EMPTY));
        assert!(!m.is_solid(1, 1));
    }

    #[test]
    fn out_of_bounds_is_solid_and_none() {
        let m = GridMap::new(2, 2);
        assert_eq!(m.get(-1, 0), None);
        assert!(m.is_solid(-1, 0));
        assert!(m.is_solid(5, 5));
    }

    #[test]
    fn user_defined_tiles_are_solid_by_default() {
        let mut m = GridMap::new(2, 2);
        m.set(0, 0, 42); // user-defined
        assert_eq!(m.get(0, 0), Some(42));
        assert!(m.is_solid(0, 0));
    }

    #[test]
    fn flat_height_map_defaults_are_zero_and_one() {
        let h = FlatHeightMap;
        assert_eq!(h.floor_height(0, 0), 0.0);
        assert_eq!(h.ceiling_height(0, 0), 1.0);
        // Out-of-bounds coordinates must not panic and should return the
        // same flat defaults.
        assert_eq!(h.floor_height(-5, 1000), 0.0);
        assert_eq!(h.ceiling_height(-5, 1000), 1.0);
    }

    struct StepHeights;
    impl HeightMap for StepHeights {
        fn floor_height(&self, x: i32, _y: i32) -> f64 {
            if x == 2 {
                0.3
            } else {
                0.0
            }
        }
        fn ceiling_height(&self, x: i32, _y: i32) -> f64 {
            if x == 2 {
                1.5
            } else {
                1.0
            }
        }
    }

    #[test]
    fn custom_height_map_works_through_trait_object() {
        let h: &dyn HeightMap = &StepHeights;
        assert_eq!(h.floor_height(0, 0), 0.0);
        assert_eq!(h.ceiling_height(0, 0), 1.0);
        assert_eq!(h.floor_height(2, 0), 0.3);
        assert_eq!(h.ceiling_height(2, 0), 1.5);
    }
}
