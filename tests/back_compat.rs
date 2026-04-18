//! Back-compatibility tests for `render_walls_with_heights`.
//!
//! `nobiscuit` v0.2.0 and other downstream users depend on `render_walls`
//! and on the implicit flat-world projection it uses. The new
//! `render_walls_with_heights` function must reduce exactly to that legacy
//! behavior when called with [`FlatHeightMap`] and a camera at the default
//! eye height (`z = 0.5`). These tests run both renderers through the real
//! raycaster on two representative maps and assert pixel-for-pixel equality.

use termray::{
    cast_ray, render_walls, render_walls_with_heights, Camera, Color, FlatHeightMap, Framebuffer,
    GridMap, HitSide, RayHit, TileMap, TileType, Vec2f, WallTexturer, TILE_EMPTY, TILE_WALL,
};

struct Solid;

impl WallTexturer for Solid {
    fn sample_wall(
        &self,
        _tile: TileType,
        wall_x: f64,
        wall_y: f64,
        side: HitSide,
        brightness: f64,
        tile_hash: u32,
    ) -> Color {
        // Use every input so a regression in any of them would show up.
        let r = ((wall_x * 255.0) as u8).wrapping_add((tile_hash & 0xff) as u8);
        let g = (wall_y * 255.0) as u8;
        let b = match side {
            HitSide::Vertical => 200,
            HitSide::Horizontal => 120,
        };
        Color::rgb(r, g, b).darken(brightness)
    }
}

fn cast_all(
    map: &dyn TileMap,
    cam: &Camera,
    num_rays: usize,
    max_depth: f64,
) -> Vec<Option<RayHit>> {
    let half_fov = cam.fov / 2.0;
    let origin = Vec2f::new(cam.x, cam.y);
    (0..num_rays)
        .map(|i| {
            let ray_angle = cam.angle - half_fov + cam.fov * (i as f64) / (num_rays as f64);
            cast_ray(map, origin, ray_angle, max_depth)
        })
        .collect()
}

fn assert_framebuffers_equal(a: &Framebuffer, b: &Framebuffer, label: &str) {
    assert_eq!(a.width(), b.width(), "{label}: framebuffer width mismatch");
    assert_eq!(
        a.height(),
        b.height(),
        "{label}: framebuffer height mismatch"
    );
    for y in 0..a.height() {
        for x in 0..a.width() {
            assert_eq!(
                a.get_pixel(x, y),
                b.get_pixel(x, y),
                "{label}: pixel mismatch at ({x}, {y})"
            );
        }
    }
}

fn render_both(
    map: &GridMap,
    cam: &Camera,
    fb_w: usize,
    fb_h: usize,
) -> (Framebuffer, Framebuffer) {
    let rays = cast_all(map, cam, fb_w, 16.0);

    let mut fb_old = Framebuffer::new(fb_w, fb_h);
    let mut fb_new = Framebuffer::new(fb_w, fb_h);
    fb_old.clear(Color::default());
    fb_new.clear(Color::default());

    render_walls(&mut fb_old, &rays, &Solid, 16.0);
    render_walls_with_heights(&mut fb_new, &rays, &Solid, &FlatHeightMap, cam, 16.0);

    (fb_old, fb_new)
}

#[test]
fn open_four_by_four_flat_heights_match_legacy() {
    let mut map = GridMap::new(4, 4);
    for x in 1..3 {
        for y in 1..3 {
            map.set(x, y, TILE_EMPTY);
        }
    }
    let cam = Camera::new(2.0, 2.0, 0.3, 70f64.to_radians());
    let (fb_old, fb_new) = render_both(&map, &cam, 64, 40);
    assert_framebuffers_equal(&fb_old, &fb_new, "open 4x4");
}

#[test]
fn single_pillar_flat_heights_match_legacy() {
    let mut map = GridMap::new(6, 6);
    for x in 1..5 {
        for y in 1..5 {
            map.set(x, y, TILE_EMPTY);
        }
    }
    // Drop one solid pillar back in the middle.
    map.set(3, 3, TILE_WALL);

    let cam = Camera::new(2.0, 2.0, 0.7, 70f64.to_radians());
    let (fb_old, fb_new) = render_both(&map, &cam, 64, 40);
    assert_framebuffers_equal(&fb_old, &fb_new, "pillar 6x6");
}

#[test]
fn larger_scene_flat_heights_match_legacy() {
    // A few scattered pillars so the framebuffer has a healthy mix of
    // wall columns at different distances plus sky / floor gaps.
    let mut map = GridMap::new(10, 10);
    for x in 1..9 {
        for y in 1..9 {
            map.set(x, y, TILE_EMPTY);
        }
    }
    for (x, y) in [(3, 3), (5, 2), (7, 6), (2, 7), (6, 4)] {
        map.set(x, y, TILE_WALL);
    }

    let cam = Camera::new(5.0, 5.0, -0.4, 70f64.to_radians());
    let (fb_old, fb_new) = render_both(&map, &cam, 80, 48);
    assert_framebuffers_equal(&fb_old, &fb_new, "scattered 10x10");
}
