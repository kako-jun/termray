//! termray — generic TUI raycasting engine.
//!
//! Provides the rendering skeleton for raycasting-based 3D views in the terminal.
//! Texture and sprite art are injected via traits so the crate itself stays free of
//! application-specific styling.
//!
//! # Reserved tile IDs
//!
//! - `0` [`TILE_EMPTY`] — walkable space
//! - `1` [`TILE_WALL`] — solid, textured wall
//! - `2` [`TILE_VOID`] — solid but invisible (map-edge / hole)
//!
//! IDs `3..=255` are user-defined. Your [`WallTexturer`] implementation decides how
//! they look and your [`TileMap::is_solid`] decides whether they block movement.

pub mod camera;
pub mod floor;
pub mod framebuffer;
pub mod map;
pub mod math;
pub mod ray;
pub mod renderer;
pub mod sprite;

pub use camera::Camera;
pub use floor::{render_floor_ceiling, FloorTexturer};
pub use framebuffer::{Color, Framebuffer};
pub use map::{
    FlatHeightMap, GridMap, HeightMap, TileMap, TileType, TILE_EMPTY, TILE_VOID, TILE_WALL,
};
pub use math::{normalize_angle, Vec2f};
pub use ray::{cast_ray, HitSide, RayHit};
pub use renderer::{render_walls, render_walls_with_heights, WallTexturer};
pub use sprite::{
    project_sprites, render_sprites, Sprite, SpriteArt, SpriteDef, SpriteRenderResult,
};
