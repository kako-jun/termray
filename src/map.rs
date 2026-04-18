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
pub trait TileMap {
    fn width(&self) -> usize;
    fn height(&self) -> usize;
    fn get(&self, x: i32, y: i32) -> Option<TileType>;
    /// Whether the tile at `(x, y)` blocks rays. Out-of-bounds should be solid.
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
