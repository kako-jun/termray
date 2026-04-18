# termray

Generic TUI raycasting engine — grid maps, DDA wall rendering, perspective floors
and ceilings, sprites with depth testing. Pure Rust, no runtime dependencies.

Designed as a shared rendering core for terminal games / tools that want a
first-person 3D view, without dictating visual style. Applications supply their
own textures and sprite art via traits.

## Status

Pre-release. `v0.1.0` targets feature parity with the internal raycaster that
powered [nobiscuit](https://github.com/kako-jun/nobiscuit) v0.1.0, minus the
application-specific styling. Slopes (#3), arbitrary-angle cameras (#4), and
sprite text labels (#5) are planned for subsequent minor releases.

## Reserved tile IDs

termray only defines three tile IDs; everything else is up to your app.

- `0` — `TILE_EMPTY`, walkable
- `1` — `TILE_WALL`, solid and textured
- `2` — `TILE_VOID`, solid but invisible (represents regions outside the playable map)

Your `TileMap::is_solid` implementation is authoritative for blocking rays.

## Quick look

```rust
use termray::{
    render_floor_ceiling, render_walls, Camera, Color, FloorTexturer, Framebuffer,
    GridMap, HitSide, TileType, WallTexturer, TILE_EMPTY, TILE_WALL,
};

struct Solid;
impl WallTexturer for Solid {
    fn sample_wall(&self, _t: TileType, _wx: f64, _wy: f64, side: HitSide, b: f64, _h: u32) -> Color {
        match side {
            HitSide::Vertical   => Color::rgb(200, 170, 140).darken(b),
            HitSide::Horizontal => Color::rgb(170, 140, 110).darken(b),
        }
    }
}
impl FloorTexturer for Solid {
    fn sample_floor(&self, _x: f64, _y: f64, b: f64)   -> Color { Color::rgb(110, 95, 75).darken(b) }
    fn sample_ceiling(&self, _x: f64, _y: f64, b: f64) -> Color { Color::rgb(60, 70, 90).darken(b) }
}

let mut map = GridMap::new(10, 10);
for x in 1..9 { for y in 1..9 { map.set(x, y, TILE_EMPTY); } }

let cam = Camera::new(5.0, 5.0, 0.0, 70f64.to_radians());
let mut fb = Framebuffer::new(80, 40);
let rays = cam.cast_all_rays(&map, fb.width(), 16.0);

render_floor_ceiling(&mut fb, &rays, &Solid, &cam);
render_walls(&mut fb, &rays, &Solid, 16.0);
```

See `examples/maze.rs` for an interactive demo:

```sh
cargo run --example maze
```

## API surface

| Module | Public items |
| --- | --- |
| `math` | `Vec2f`, `normalize_angle` |
| `framebuffer` | `Color`, `Framebuffer` |
| `map` | `TileType`, `TILE_EMPTY`, `TILE_WALL`, `TILE_VOID`, `TileMap`, `GridMap` |
| `ray` | `RayHit`, `HitSide`, `cast_ray` |
| `camera` | `Camera` |
| `renderer` | `WallTexturer`, `render_walls`, `tile_hash`, `WALL_HEIGHT_SCALE` |
| `floor` | `FloorTexturer`, `render_floor_ceiling` |
| `sprite` | `Sprite`, `SpriteDef`, `SpriteArt`, `SpriteRenderResult`, `project_sprites`, `render_sprites` |

## Relationship to nobiscuit-engine

`nobiscuit-engine` v0.1.0 on crates.io was an early draft that mixed nobiscuit's
Japanese-house textures (fusuma, shoji, tatami floor, biscuit sprites) into the
engine. That crate is frozen at 0.1.0 and superseded by termray — nobiscuit
itself will migrate to depend on termray and keep its textures in-app where
they belong.

## License

MIT
