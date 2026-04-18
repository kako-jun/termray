# termray

Generic TUI raycasting engine — grid maps, DDA wall rendering, perspective floors
and ceilings, sprites with depth testing. Pure Rust, no runtime dependencies.

Designed as a shared rendering core for terminal games / tools that want a
first-person 3D view, without dictating visual style. Applications supply their
own textures and sprite art via traits.

## Status

Pre-release. `v0.1.0` targets feature parity with the internal raycaster that
powered [nobiscuit](https://github.com/kako-jun/nobiscuit) v0.1.0, minus the
application-specific styling. Arbitrary-angle cameras (#4) and stepped
heightmaps (#3 Phase 1) landed in `v0.2.0` — wall tops/bottoms follow a
per-tile `HeightMap` and the camera carries an eye-height `z`. Corner-
interpolated true slopes (with `Camera.pitch` and non-flat floor projection)
are tracked in #8 for `v0.3.0`. Sprite text labels (#5) come after.

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

See `examples/maze.rs` for a keystroke-driven interactive demo,
`examples/free_camera.rs` for a physics-style demo with velocity, friction,
and strafe controls, and `examples/terrain.rs` for a stepped-heightmap demo
where the camera's eye height follows the floor as you walk across tiles of
different elevation:

```sh
cargo run --example maze
cargo run --example free_camera
cargo run --example terrain
```

## Free-angle camera (physics integration)

`Camera` stores its pose as `(x: f64, y: f64, angle: f64)` with no grid
snapping, so it is happy to accept sub-unit positions and arbitrary yaw from
an external physics engine. The recommended seam is to keep velocity /
angular state outside the camera and push new poses in each frame:

```rust
# use termray::Camera;
# let mut cam = Camera::new(0.0, 0.0, 0.0, 70f64.to_radians());
# let (mut vx, mut vy, mut yaw) = (0.0_f64, 0.0_f64, 0.0_f64);
# let dt = 1.0 / 60.0;
// Every frame, after your rapier3d / custom integrator has produced a new pose:
let new_x = cam.x + vx * dt;
let new_y = cam.y + vy * dt;
cam.set_pose(new_x, new_y, yaw);

// Strafe / velocity math can lean on the unit direction vectors:
let fwd = cam.forward();     // (cos(yaw), sin(yaw))
let right = cam.right();     // forward rotated +90°
vx += (fwd.x + right.x) * dt;
vy += (fwd.y + right.y) * dt;
```

`set_position` and `set_yaw` are the corresponding single-axis setters, for
cases where only one component changes per update.

## Heightmaps (Phase 1 — stepped heights)

`v0.2.0` introduces a `HeightMap` trait that lets walls vary in vertical
extent per tile, and an eye-height `z` on `Camera` so the viewer can stand
at different elevations. `render_walls_with_heights` is the new renderer
that consults both. The existing `render_walls` / `render_floor_ceiling`
keep their flat-world behavior untouched — nothing on the old path
regresses.

```rust
use termray::{
    render_walls_with_heights, Camera, FlatHeightMap, HeightMap,
};
# use termray::{
#     render_floor_ceiling, Color, FloorTexturer, Framebuffer, GridMap,
#     HitSide, TileType, WallTexturer, TILE_EMPTY, TILE_WALL,
# };
# struct Solid;
# impl WallTexturer for Solid {
#     fn sample_wall(&self, _t: TileType, _wx: f64, _wy: f64, side: HitSide, b: f64, _h: u32) -> Color {
#         match side {
#             HitSide::Vertical   => Color::rgb(200, 170, 140).darken(b),
#             HitSide::Horizontal => Color::rgb(170, 140, 110).darken(b),
#         }
#     }
# }
# impl FloorTexturer for Solid {
#     fn sample_floor(&self, _x: f64, _y: f64, b: f64)   -> Color { Color::rgb(110, 95, 75).darken(b) }
#     fn sample_ceiling(&self, _x: f64, _y: f64, b: f64) -> Color { Color::rgb(60, 70, 90).darken(b) }
# }

struct MyHeights;
impl HeightMap for MyHeights {
    fn ceiling_height(&self, x: i32, _y: i32) -> f64 {
        // Short fence in the eastern columns, full-height walls elsewhere.
        if x >= 6 { 0.4 } else { 1.0 }
    }
}

# let mut map = GridMap::new(10, 10);
# for x in 1..9 { for y in 1..9 { map.set(x, y, TILE_EMPTY); } }
let mut cam = Camera::with_z(5.0, 5.0, 0.5, 0.0, 70f64.to_radians());
let mut fb = Framebuffer::new(80, 40);
let rays = cam.cast_all_rays(&map, fb.width(), 16.0);

// Floor/ceiling still use the flat-world renderer in Phase 1.
render_floor_ceiling(&mut fb, &rays, &Solid, &cam);
// Walls now consult per-tile heights and the camera's eye height.
render_walls_with_heights(&mut fb, &rays, &Solid, &MyHeights, &cam, 16.0);

// When the player steps onto a raised tile, lift the camera with them:
let floor_here = MyHeights.floor_height(cam.x.floor() as i32, cam.y.floor() as i32);
cam.set_z(floor_here + 0.5);
```

Phase 1 deliberately limits itself to **stepped** heights — wall tops and
bottoms snap to the tile's `floor_height` / `ceiling_height`, and
`render_floor_ceiling` still paints a flat horizontal plane. True
corner-interpolated slopes, `Camera.pitch`, and ray-floor intersection are
tracked separately in [#8](https://github.com/kako-jun/termray/issues/8)
for `v0.3.0` (the release street-golf will depend on for SRTM terrain).

## API surface

| Module | Public items |
| --- | --- |
| `math` | `Vec2f`, `normalize_angle` |
| `framebuffer` | `Color`, `Framebuffer` |
| `map` | `TileType`, `TILE_EMPTY`, `TILE_WALL`, `TILE_VOID`, `TileMap`, `GridMap`, `HeightMap`, `FlatHeightMap` |
| `ray` | `RayHit`, `HitSide`, `cast_ray` |
| `camera` | `Camera` (incl. `with_z`, `set_pose`, `set_pose_z`, `set_position`, `set_yaw`, `set_z`, `forward`, `right`) |
| `renderer` | `WallTexturer`, `render_walls`, `render_walls_with_heights`, `tile_hash`, `WALL_HEIGHT_SCALE` |
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
