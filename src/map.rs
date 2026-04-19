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

/// Per-cell floor and ceiling corner heights.
///
/// # Corner ordering
///
/// All four-element arrays use the same ordering: `[NW, NE, SW, SE]`.
/// Within the world grid the corners of cell `(x, y)` are at:
///
/// - `NW` → world `(x,   y  )`
/// - `NE` → world `(x+1, y  )`
/// - `SW` → world `(x,   y+1)`
/// - `SE` → world `(x+1, y+1)`
///
/// (y increases southward in termray's world coordinates, matching how
/// [`crate::ray::cast_ray`] walks the grid.)
///
/// # Continuity contract
///
/// Adjacent cells share corners. A `HeightMap` implementation **should**
/// return matching values on the shared corners of neighboring cells —
/// e.g. cell `(x, y)`'s `NE` and cell `(x+1, y)`'s `NW` both refer to the
/// same world corner `(x+1, y)` and should be equal. The renderer does not
/// panic when this contract is violated, but a mismatch produces a visible
/// seam ("tear") at the shared cell edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CornerHeights {
    /// Floor heights at the four corners, in `[NW, NE, SW, SE]` order.
    pub floor: [f64; 4],
    /// Ceiling heights at the four corners, in `[NW, NE, SW, SE]` order.
    pub ceil: [f64; 4],
}

/// Array index of the NW corner inside a `[f64; 4]` from [`CornerHeights`].
pub const CORNER_NW: usize = 0;
/// Array index of the NE corner inside a `[f64; 4]` from [`CornerHeights`].
pub const CORNER_NE: usize = 1;
/// Array index of the SW corner inside a `[f64; 4]` from [`CornerHeights`].
pub const CORNER_SW: usize = 2;
/// Array index of the SE corner inside a `[f64; 4]` from [`CornerHeights`].
pub const CORNER_SE: usize = 3;

impl CornerHeights {
    /// Construct a [`CornerHeights`] whose floor and ceiling are both flat
    /// (all four corners at the same height).
    ///
    /// This is the fast path for tile-flat worlds: `CornerHeights::flat(0.0, 1.0)`
    /// reproduces the implicit projection of pre-v0.3 termray.
    pub const fn flat(floor: f64, ceil: f64) -> Self {
        Self {
            floor: [floor; 4],
            ceil: [ceil; 4],
        }
    }

    /// Bilinear sample of the floor surface at local coordinates `(u, v)`.
    ///
    /// `u` maps from the west edge (0.0) to the east edge (1.0) of the cell.
    /// `v` maps from the north edge (0.0) to the south edge (1.0).
    /// Values outside [0, 1] are not clamped — the same bilinear formula
    /// extrapolates, which is useful for ray-intersection math that may
    /// sample slightly past a corner due to floating-point error.
    pub fn sample_floor(&self, u: f64, v: f64) -> f64 {
        bilinear(&self.floor, u, v)
    }

    /// Bilinear sample of the ceiling surface at local coordinates `(u, v)`.
    /// See [`CornerHeights::sample_floor`] for the coordinate convention.
    pub fn sample_ceil(&self, u: f64, v: f64) -> f64 {
        bilinear(&self.ceil, u, v)
    }
}

fn bilinear(corners: &[f64; 4], u: f64, v: f64) -> f64 {
    // corners = [NW, NE, SW, SE] with NW at (u=0, v=0), SE at (u=1, v=1).
    let nw = corners[CORNER_NW];
    let ne = corners[CORNER_NE];
    let sw = corners[CORNER_SW];
    let se = corners[CORNER_SE];
    let top = nw * (1.0 - u) + ne * u;
    let bot = sw * (1.0 - u) + se * u;
    top * (1.0 - v) + bot * v
}

impl Default for CornerHeights {
    /// Flat unit cell: floor at `0.0`, ceiling at `1.0`.
    fn default() -> Self {
        Self::flat(0.0, 1.0)
    }
}

/// World-space floor and ceiling heights per cell corner.
///
/// Returns four corner heights for both floor and ceiling — see
/// [`CornerHeights`] for the `[NW, NE, SW, SE]` ordering convention and the
/// continuity contract between adjacent cells.
///
/// The default implementation returns [`CornerHeights::flat(0.0, 1.0)`],
/// which reproduces pre-v0.3 termray's implicit projection. Wrap your
/// `HeightMap` around data of your choice (per-tile arrays, SRTM samples,
/// procedural noise) to vary heights.
///
/// # Coordinate contract
///
/// Coordinates can be out-of-bounds; implementations should return sane
/// defaults (typically `CornerHeights::flat(0.0, 1.0)`) rather than panic.
/// termray calls `cell_heights` for every rendered wall column and every
/// walked cell in the floor / ceiling renderer.
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
/// Implementations should guarantee `ceil[i] >= floor[i]` at every corner.
/// Violations don't panic — the renderer silently skips columns where the
/// projected wall inverts — but the resulting picture is undefined.
pub trait HeightMap {
    /// Corner heights at cell `(x, y)`.
    fn cell_heights(&self, _x: i32, _y: i32) -> CornerHeights {
        CornerHeights::flat(0.0, 1.0)
    }
}

/// Zero-sized [`HeightMap`] implementing a fully flat world
/// (floor=0.0, ceiling=1.0 at every corner).
///
/// Use this when you don't need per-tile height variation — the resulting
/// projection matches pre-v0.3 termray's implicit flat-world behavior.
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
        let ch = h.cell_heights(0, 0);
        assert_eq!(ch.floor, [0.0; 4]);
        assert_eq!(ch.ceil, [1.0; 4]);
        // Out-of-bounds coordinates must not panic and should return the
        // same flat defaults.
        let oob = h.cell_heights(-5, 1000);
        assert_eq!(oob, CornerHeights::flat(0.0, 1.0));
    }

    #[test]
    fn corner_heights_flat_sets_all_four_corners() {
        let ch = CornerHeights::flat(0.25, 1.75);
        assert_eq!(ch.floor, [0.25; 4]);
        assert_eq!(ch.ceil, [1.75; 4]);
    }

    #[test]
    fn corner_heights_bilinear_samples_as_expected() {
        // Tilted floor rising east-to-south: NW=0, NE=0.5, SW=0.5, SE=1.0.
        let ch = CornerHeights {
            floor: [0.0, 0.5, 0.5, 1.0],
            ceil: [1.0, 1.0, 1.0, 1.0],
        };
        // Corners: NW, NE, SW, SE.
        assert!((ch.sample_floor(0.0, 0.0) - 0.0).abs() < 1e-12);
        assert!((ch.sample_floor(1.0, 0.0) - 0.5).abs() < 1e-12);
        assert!((ch.sample_floor(0.0, 1.0) - 0.5).abs() < 1e-12);
        assert!((ch.sample_floor(1.0, 1.0) - 1.0).abs() < 1e-12);
        // Center = average of four corners.
        assert!((ch.sample_floor(0.5, 0.5) - 0.5).abs() < 1e-12);
    }

    struct StepHeights;
    impl HeightMap for StepHeights {
        fn cell_heights(&self, x: i32, _y: i32) -> CornerHeights {
            if x == 2 {
                CornerHeights::flat(0.3, 1.5)
            } else {
                CornerHeights::flat(0.0, 1.0)
            }
        }
    }

    #[test]
    fn custom_height_map_works_through_trait_object() {
        let h: &dyn HeightMap = &StepHeights;
        let at0 = h.cell_heights(0, 0);
        assert_eq!(at0.floor, [0.0; 4]);
        assert_eq!(at0.ceil, [1.0; 4]);
        let at2 = h.cell_heights(2, 0);
        assert_eq!(at2.floor, [0.3; 4]);
        assert_eq!(at2.ceil, [1.5; 4]);
    }
}
