# termray

Generic TUI raycasting engine — grid maps, DDA wall rendering, perspective floors
and ceilings, sprites with depth testing. Pure Rust, no runtime dependencies.

Designed as a shared rendering core for terminal games / tools that want a
first-person 3D view, without dictating visual style. Applications supply their
own textures and sprite art via traits.

## Status

`v0.3.0` unifies the height / pitch / slope pipeline behind a single
`CornerHeights` + `HitFace` + `render_walls` / `render_floor_ceiling` pair.
Walls, floors, sprites, and labels all consult the same `HeightMap` and
the same `Camera` (now carrying a `pitch` horizon shift), so continuous
slopes, bowls, and ramps render consistently across every layer. This is a
breaking release with **no** back-compat shim — see `CHANGELOG.md` for the
migration list.

Earlier milestones: arbitrary-angle cameras (#4) and stepped heightmaps
(#3 Phase 1) landed in `v0.2.0`; world-anchored text labels on sprites
(#5) landed mid-0.2.x; true corner-interpolated slopes, `Camera::pitch`
and non-flat floor projection ship now as part of (#8).

## Reserved tile IDs

termray only defines three tile IDs; everything else is up to your app.

- `0` — `TILE_EMPTY`, walkable
- `1` — `TILE_WALL`, solid and textured
- `2` — `TILE_VOID`, solid but invisible (represents regions outside the playable map)

Your `TileMap::is_solid` implementation is authoritative for blocking rays.

## Quick look

```rust
use termray::{
    render_floor_ceiling, render_walls, Camera, Color, FlatHeightMap, FloorTexturer, Framebuffer,
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

// `FlatHeightMap` gives you the pre-v0.3 flat-world look; swap in any
// `HeightMap` implementation to vary floor / ceiling heights per corner.
render_floor_ceiling(&mut fb, &rays, &Solid, &FlatHeightMap, &cam, 16.0);
render_walls(&mut fb, &rays, &Solid, &FlatHeightMap, &cam, 16.0);
```

See `examples/maze.rs` for a keystroke-driven interactive demo,
`examples/free_camera.rs` for a physics-style demo with velocity, friction,
and strafe controls, `examples/terrain.rs` for a stepped-heightmap demo
where the camera's eye height follows the floor, and `examples/slope.rs`
for a continuous-slope demo with smooth hills and valleys plus `r` / `f`
pitch controls:

```sh
cargo run --example maze
cargo run --example free_camera
cargo run --example terrain
cargo run --example slope
cargo run --example labeled_sprites
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

### Pitch convention

`Camera::pitch` is a **horizon offset** (Doom / Heretic style), not a 3D
rotation.

- `pitch > 0` means **looking up**. The horizon slides *down* on screen
  (the sky takes up more of the view).
- `pitch < 0` means **looking down**. The horizon slides *up*.

Internally the offset is applied as a single `tan(pitch) * focal_px` pixel
shift shared by walls, floor / ceiling, sprites, and labels — so pitch only
ever changes *where* the horizon sits, never how 3D the scene looks. Keep
`|pitch|` strictly less than `FRAC_PI_2` to avoid the `tan` singularity at
the poles; values up to about `0.9` are visually useful, past that the
view fills almost entirely with sky or floor.

## Heightmaps (v0.3.0 — corner-interpolated slopes)

The `HeightMap` trait exposes one method, `cell_heights(x, y) ->
CornerHeights`. Each `CornerHeights` carries four floor and four ceiling
heights in `[NW, NE, SW, SE]` order; walls, the floor/ceiling renderer,
sprites, and labels all consult the same data, so a slope is a slope
everywhere on screen.

The default implementation returns `CornerHeights::flat(0.0, 1.0)` — an
empty `impl HeightMap for MyType {}` still gives you the pre-v0.3 flat
world.

```rust
use termray::{
    render_floor_ceiling, render_walls, Camera, CornerHeights, FlatHeightMap, HeightMap,
};
# use termray::{
#     Color, FloorTexturer, Framebuffer, GridMap, HitSide, TileType,
#     WallTexturer, TILE_EMPTY, TILE_WALL,
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

/// Slope rising toward +x: floor height at world corner (cx, cy) is 0.1 * cx.
/// Using a world-corner function guarantees the continuity contract —
/// adjacent cells that share a corner return the same value from either side.
struct Ramp;
impl HeightMap for Ramp {
    fn cell_heights(&self, x: i32, y: i32) -> CornerHeights {
        let at = |cx: i32, _cy: i32| 0.1 * cx as f64;
        CornerHeights {
            floor: [at(x, y), at(x + 1, y), at(x, y + 1), at(x + 1, y + 1)],
            ceil: [1.0; 4],
        }
    }
}

# let mut map = GridMap::new(10, 10);
# for x in 1..9 { for y in 1..9 { map.set(x, y, TILE_EMPTY); } }
let mut cam = Camera::with_z(5.0, 5.0, 0.5, 0.0, 70f64.to_radians());
cam.set_pitch(0.1); // look slightly up — uniform horizon shift for walls/floor/sprites
let mut fb = Framebuffer::new(80, 40);
let rays = cam.cast_all_rays(&map, fb.width(), 16.0);

render_floor_ceiling(&mut fb, &rays, &Solid, &Ramp, &cam, 16.0);
render_walls(&mut fb, &rays, &Solid, &Ramp, &cam, 16.0);

// Track the slope under the player: sample the floor bilinearly at the
// exact camera position for smooth vertical motion.
let cx = cam.x.floor() as i32;
let cy = cam.y.floor() as i32;
let u = cam.x - cx as f64;
let v = cam.y - cy as f64;
let floor_here = Ramp.cell_heights(cx, cy).sample_floor(u, v);
cam.set_z(floor_here + 0.5);
```

**Continuity contract.** Cells share corners with their neighbours. For
continuous ground the shared corner values must agree between the two
cells — writing your heightmap as a world-corner function (like `Ramp`
above) is the easiest way to guarantee that. Non-matching corners are
allowed and render as intentional step edges (`examples/terrain.rs`).

See `examples/slope.rs` for a hill + bowl demo and
[#8](https://github.com/kako-jun/termray/issues/8) for the design notes.

## Sprite text labels

A [`Label`](src/label.rs) is a world-anchored text entity independent of
[`Sprite`](src/sprite.rs). Place both at the same `(x, y)` to compose an
"icon with caption" — the primary use case is friendly-filer, where the
file name is the real content and the icon is just a visual anchor.

```rust
use termray::{Color, Font8x8, Label, project_labels, render_labels};
# use termray::{Framebuffer, RayHit};
# let mut fb = Framebuffer::new(80, 40);
# let rays: Vec<Option<RayHit>> = vec![None; 80];
# let cam_x = 0.0; let cam_y = 0.0; let cam_angle = 0.0;
# let fov = 70f64.to_radians();

let labels = vec![Label {
    text: "README.md".into(),
    x: 5.0,
    y: 3.0,
    world_height: 0.85,                // roughly head-height above the floor
    color: Color::rgb(240, 240, 240),
    background: Some(Color::rgb(20, 20, 25)),
    max_chars: Some(12),               // greedy word-wrap on whitespace
}];

# use termray::{Camera, FlatHeightMap};
# let cam = Camera::new(cam_x, cam_y, cam_angle, fov);
// `project_labels` takes the same camera + heightmap you render the world
// with, so captions inherit both the pitch horizon shift and the slope
// under their anchor point.
let projected = project_labels(&labels, &cam, &FlatHeightMap, fb.width(), fb.height());
render_labels(&mut fb, &projected, &rays, &Font8x8, 16.0);
```

Glyphs render at the font's native pixel size (no distance scaling), so
labels stay readable near and far — the right trade-off when the label
content, not the sprite, is what the user actually reads. Occlusion uses
two granularities: glyphs are hidden wholesale if any of their columns are
behind a wall (keeping glyph edges clean at corners), while the optional
background rectangle is blended per-column.

The bundled [`Font8x8`] covers `basic_latin` (0x20..=0x7E). For non-Latin
content (Japanese filenames for friendly-filer, CJK labels in general), ship
your own [`GlyphRenderer`] implementation — any bitmap font works, termray
doesn't care about the source.

See `examples/labeled_sprites.rs` for a friendly-filer–style demo where file
icons carry captions that occlude correctly when the camera moves behind the
interior wall.

## API surface

| Module | Public items |
| --- | --- |
| (crate root) | `MIN_PROJECTION_DISTANCE` (near-cut shared by `project_sprites` / `project_labels`) |
| `math` | `Vec2f`, `normalize_angle` |
| `framebuffer` | `Color`, `Framebuffer` |
| `map` | `TileType`, `TILE_EMPTY`, `TILE_WALL`, `TILE_VOID`, `TileMap`, `GridMap`, `HeightMap`, `FlatHeightMap`, `CornerHeights`, `CORNER_NW`/`NE`/`SW`/`SE` |
| `ray` | `RayHit`, `HitSide`, `HitFace`, `cast_ray` |
| `camera` | `Camera` (incl. `with_z`, `set_pose`, `set_position`, `set_yaw`, `set_z`, `set_pitch`, `forward`, `right`) |
| `renderer` | `WallTexturer`, `render_walls`, `tile_hash`, `WALL_HEIGHT_SCALE` |
| `floor` | `FloorTexturer`, `render_floor_ceiling` |
| `sprite` | `Sprite`, `SpriteDef`, `SpriteArt`, `SpriteRenderResult` (with `screen_y_feet: f64`), `project_sprites`, `render_sprites` |
| `label` | `Label`, `ProjectedLabel` (with `screen_y_baseline: f64`), `GlyphRenderer`, `Font8x8`, `project_labels`, `render_labels` |

## Relationship to nobiscuit-engine

`nobiscuit-engine` v0.1.0 on crates.io was an early draft that mixed nobiscuit's
Japanese-house textures (fusuma, shoji, tatami floor, biscuit sprites) into the
engine. That crate is frozen at 0.1.0 and superseded by termray — nobiscuit
itself will migrate to depend on termray and keep its textures in-app where
they belong.

## License

MIT
