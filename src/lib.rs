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

// Doc quality gate: keep rustdoc intra-doc links honest across the crate.
// A broken link is almost always a rename that lost its reference — fail the
// build rather than shipping rotten docs.
#![deny(rustdoc::broken_intra_doc_links)]

pub mod camera;
pub mod floor;
pub mod framebuffer;
pub mod label;
pub mod map;
pub mod math;
pub mod ray;
pub mod renderer;
pub mod sprite;

pub use camera::Camera;
pub use floor::{FloorTexturer, render_floor_ceiling};
pub use framebuffer::{Color, Framebuffer};
pub use label::{Font8x8, GlyphRenderer, Label, ProjectedLabel, project_labels, render_labels};
pub use map::{
    CORNER_NE, CORNER_NW, CORNER_SE, CORNER_SW, CornerHeights, FlatHeightMap, GridMap, HeightMap,
    TILE_EMPTY, TILE_VOID, TILE_WALL, TileMap, TileType,
};
pub use math::{Vec2f, normalize_angle};
pub use ray::{HitFace, HitSide, RayHit, cast_ray};
pub use renderer::{WALL_HEIGHT_SCALE, WallTexturer, render_walls, tile_hash};
pub use sprite::{
    Sprite, SpriteArt, SpriteDef, SpriteRenderResult, project_sprites, render_sprites,
};
