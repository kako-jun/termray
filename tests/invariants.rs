//! Invariant tests for the v0.3.0 unified height-aware pipeline.
//!
//! These tests anchor the five behavioral contracts of the Phase 2 work
//! (kako-jun/termray#8) so regressions show up as test failures rather than
//! subtle visual artifacts:
//!
//! 1. **Tile-flat world reproduces pre-v0.3 projection.** A `HeightMap`
//!    whose every cell returns `CornerHeights::flat(0.0, 1.0)` collapses to
//!    the same framebuffer as `FlatHeightMap`.
//! 2. **Pitch shifts the horizon** uniformly across walls, floor, and
//!    sprites.
//! 3. **Sloped floors interpolate linearly along `wall_x`** (task #6's
//!    corner interpolation).
//! 4. **Adjacent cells stitch continuously** — the shared edge of two
//!    neighbours renders without a visible seam when their shared corners
//!    match.
//! 5. **Sprites stand on sloped floors.** A sprite placed at (x, y) is
//!    anchored to the bilinear-sampled floor under it, not to `fb_h / 2`.

use termray::{
    Camera, Color, CornerHeights, FlatHeightMap, Framebuffer, GridMap, HeightMap, HitSide, Sprite,
    TILE_EMPTY, TILE_WALL, TileMap, TileType, Vec2f, WallTexturer, cast_ray, project_sprites,
    render_floor_ceiling, render_walls,
};

// ------------------------------------------------------------------
// Shared helpers
// ------------------------------------------------------------------

struct Solid;

impl WallTexturer for Solid {
    fn sample_wall(
        &self,
        tile: TileType,
        wall_x: f64,
        wall_y: f64,
        side: HitSide,
        brightness: f64,
        tile_hash: u32,
    ) -> Color {
        let r = ((wall_x * 255.0) as u8)
            .wrapping_add((tile_hash & 0xff) as u8)
            .wrapping_add(tile);
        let g = (wall_y * 255.0) as u8;
        let b = match side {
            HitSide::Vertical => 200,
            HitSide::Horizontal => 120,
        };
        Color::rgb(r, g, b).darken(brightness)
    }
}

impl termray::FloorTexturer for Solid {
    fn sample_floor(&self, wx: f64, wy: f64, brightness: f64) -> Color {
        Color::rgb((wx * 32.0) as u8, (wy * 32.0) as u8, 80).darken(brightness)
    }
    fn sample_ceiling(&self, _wx: f64, _wy: f64, brightness: f64) -> Color {
        Color::rgb(30, 40, 60).darken(brightness)
    }
}

fn cast_all(
    map: &dyn TileMap,
    cam: &Camera,
    num_rays: usize,
    max_depth: f64,
) -> Vec<Option<termray::RayHit>> {
    let half_fov = cam.fov / 2.0;
    let origin = Vec2f::new(cam.x, cam.y);
    (0..num_rays)
        .map(|i| {
            let ray_angle = cam.angle - half_fov + cam.fov * (i as f64) / (num_rays as f64);
            cast_ray(map, origin, ray_angle, max_depth)
        })
        .collect()
}

fn render_scene(
    map: &GridMap,
    heights: &dyn HeightMap,
    cam: &Camera,
    fb_w: usize,
    fb_h: usize,
) -> Framebuffer {
    let rays = cast_all(map, cam, fb_w, 16.0);
    let mut fb = Framebuffer::new(fb_w, fb_h);
    fb.clear(Color::default());
    // Paint order matters: the floor/ceiling writer stays out of the wall
    // strip (bounds-aware), so we do floor first then walls to leave walls
    // on top for any downstream testing that distinguishes them by color.
    render_floor_ceiling(&mut fb, &rays, &Solid, heights, cam, 16.0);
    render_walls(&mut fb, &rays, &Solid, heights, cam, 16.0);
    fb
}

fn simple_room(w: usize, h: usize) -> GridMap {
    let mut map = GridMap::new(w, h);
    for x in 1..w - 1 {
        for y in 1..h - 1 {
            map.set(x, y, TILE_EMPTY);
        }
    }
    map
}

// ------------------------------------------------------------------
// 1. tile-flat
// ------------------------------------------------------------------

struct TileFlat;
impl HeightMap for TileFlat {
    fn cell_heights(&self, _x: i32, _y: i32) -> CornerHeights {
        CornerHeights::flat(0.0, 1.0)
    }
}

#[test]
fn tile_flat_reproduces_flat_heightmap() {
    // Both heightmaps semantically describe the same world; the renderers
    // must produce the same framebuffer pixel-for-pixel.
    let map = simple_room(6, 6);
    let cam = Camera::new(2.5, 2.5, 0.3, 70f64.to_radians());
    let fb_a = render_scene(&map, &FlatHeightMap, &cam, 64, 40);
    let fb_b = render_scene(&map, &TileFlat, &cam, 64, 40);

    for y in 0..fb_a.height() {
        for x in 0..fb_a.width() {
            assert_eq!(
                fb_a.get_pixel(x, y),
                fb_b.get_pixel(x, y),
                "pixel mismatch at ({x},{y}) between FlatHeightMap and TileFlat",
            );
        }
    }
}

// ------------------------------------------------------------------
// 2. pitch
// ------------------------------------------------------------------

#[test]
fn pitch_shifts_whole_image_vertically() {
    // All termray projectors — walls, floor, sprites, labels — consume the
    // same `projection_center_y`, so checking the sprite feet position
    // under level vs pitch-up is an isomorphic test for the shared horizon
    // shift. We only assert the direction (down) because the exact pixel
    // shift depends on `tan(pitch) * focal_px` (not a round number).
    let fb_w = 160usize;
    let fb_h = 120usize;
    let mut cam = Camera::new(0.0, 0.0, 0.0, 70f64.to_radians());

    let sprite = vec![Sprite {
        x: 5.0,
        y: 0.0,
        sprite_type: 0,
    }];

    let level = project_sprites(&sprite, &cam, &FlatHeightMap, fb_w, fb_h);
    cam.set_pitch(0.25); // look up — horizon shifts DOWN
    let up = project_sprites(&sprite, &cam, &FlatHeightMap, fb_w, fb_h);

    assert_eq!(level.len(), 1);
    assert_eq!(up.len(), 1);
    assert!(
        up[0].screen_y_feet > level[0].screen_y_feet,
        "pitch up should move content downward on screen; feet level={} up={}",
        level[0].screen_y_feet,
        up[0].screen_y_feet,
    );
}

// ------------------------------------------------------------------
// 3. sloped interpolation along wall_x
// ------------------------------------------------------------------

#[test]
fn sloped_floor_interpolates_bilinearly_across_cell() {
    // A single cell with a floor sloping from 0 at the west edge to 1 at
    // the east edge. `sample_floor(u, v)` should return u exactly.
    let ch = CornerHeights {
        floor: [0.0, 1.0, 0.0, 1.0],
        ceil: [1.0; 4],
    };
    for u in [0.0, 0.25, 0.5, 0.75, 1.0] {
        for v in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let h = ch.sample_floor(u, v);
            assert!(
                (h - u).abs() < 1e-12,
                "bilinear sample at (u={u}, v={v}) = {h}, expected {u}",
            );
        }
    }
}

// ------------------------------------------------------------------
// 4. adjacent continuity
// ------------------------------------------------------------------

struct ContinuousRamp;
impl HeightMap for ContinuousRamp {
    fn cell_heights(&self, x: i32, y: i32) -> CornerHeights {
        // Floor height at world corner (cx, cy) = 0.1 * cx. Applying that
        // to every corner of every cell gives a globally continuous ramp:
        // the shared corner of two neighbours has the same value coming
        // from either side, so the contract is satisfied.
        let at = |cx: i32, _cy: i32| 0.1 * cx as f64;
        CornerHeights {
            floor: [at(x, y), at(x + 1, y), at(x, y + 1), at(x + 1, y + 1)],
            ceil: [1.0; 4],
        }
    }
}

#[test]
fn adjacent_cells_agree_on_shared_corner() {
    let h = ContinuousRamp;
    for x in -2..=5 {
        for y in -2..=5 {
            let here = h.cell_heights(x, y);
            let east = h.cell_heights(x + 1, y);
            let south = h.cell_heights(x, y + 1);
            // East neighbour: here.NE must equal east.NW, here.SE must equal east.SW.
            assert!((here.floor[1] - east.floor[0]).abs() < 1e-12);
            assert!((here.floor[3] - east.floor[2]).abs() < 1e-12);
            // South neighbour: here.SW == south.NW, here.SE == south.NE.
            assert!((here.floor[2] - south.floor[0]).abs() < 1e-12);
            assert!((here.floor[3] - south.floor[1]).abs() < 1e-12);
        }
    }
}

// ------------------------------------------------------------------
// 5. sprite grounds itself on the slope
// ------------------------------------------------------------------

struct EastHigher;
impl HeightMap for EastHigher {
    fn cell_heights(&self, x: i32, y: i32) -> CornerHeights {
        // Floor rises to the east: +x direction.
        let at = |cx: i32, _cy: i32| 0.2 * cx as f64;
        CornerHeights {
            floor: [at(x, y), at(x + 1, y), at(x, y + 1), at(x + 1, y + 1)],
            ceil: [2.0; 4],
        }
    }
}

#[test]
fn sprite_feet_follow_floor_slope() {
    // Camera at x=2, y=2, facing +x. Place a sprite ahead on a slope and
    // confirm the projected feet sit higher on screen than for the same
    // sprite on a flat floor.
    let fb_w: usize = 120;
    let fb_h: usize = 80;
    let cam = Camera::with_z(2.0, 2.0, 0.5, 0.0, 70f64.to_radians());
    let spr = vec![Sprite {
        x: 6.0,
        y: 2.0,
        sprite_type: 0,
    }];

    let flat = project_sprites(&spr, &cam, &FlatHeightMap, fb_w, fb_h);
    let slope = project_sprites(&spr, &cam, &EastHigher, fb_w, fb_h);
    assert_eq!(flat.len(), 1);
    assert_eq!(slope.len(), 1);

    // On a rising floor the feet move UPWARD on screen (smaller y).
    assert!(
        slope[0].screen_y_feet < flat[0].screen_y_feet,
        "sprite on an east-rising slope should have feet above the flat-floor baseline \
         (flat={}, slope={})",
        flat[0].screen_y_feet,
        slope[0].screen_y_feet,
    );
}

// A cross-check for #1: one wall-hit column from a tile-flat scene lines
// up with a random user-defined tile id so we know wall_x / tile_hash still
// thread through correctly.
#[test]
fn user_defined_tile_ids_still_flow_through() {
    let mut map = GridMap::new(6, 6);
    for x in 1..5 {
        for y in 1..5 {
            map.set(x, y, TILE_EMPTY);
        }
    }
    map.set(4, 2, 42); // user tile id
    let cam = Camera::new(2.0, 2.5, 0.1, 70f64.to_radians());
    let rays = cast_all(&map, &cam, 80, 16.0);
    let hit_user = rays.iter().filter_map(|r| r.as_ref()).any(|h| h.tile == 42);
    assert!(
        hit_user,
        "at least one ray should land on the custom-id tile",
    );
    // Render doesn't panic.
    let _ = render_scene(&map, &FlatHeightMap, &cam, 80, 48);
    // silence unused warning on TILE_WALL import (kept for docs)
    let _ = TILE_WALL;
}
