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
}
