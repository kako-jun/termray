//! Stepped-heights demo.
//!
//! A 12x12 open arena populated with walls of different shapes — a tall
//! tower, a low fence, and a sunken floor — all driven by a single
//! [`HeightMap`] implementation. As you walk around, the camera's eye
//! height tracks the floor height of the tile it stands on, so stepping
//! into a sunken pit visibly lowers the horizon.
//!
//! Run with `cargo run --example terrain`. Controls:
//! - `w` / `s` — accelerate forward / back along the camera's facing
//! - `a` / `d` — strafe left / right (perpendicular to facing)
//! - `q` / `e` — turn left / right
//! - `esc` — quit
//!
//! NOTE (Phase 1): floor and ceiling pixels are still drawn by the existing
//! `render_floor_ceiling`, which assumes a perfectly flat world. The
//! perspective seam between flat floor pixels and stepped walls is the
//! expected Phase 1 limitation; true per-tile floors/ceilings are tracked
//! in termray#8.

use std::io::{Write, stdout};
use std::time::{Duration, Instant};

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{Event, KeyCode, poll, read};
use crossterm::execute;
use crossterm::style::{
    Color as CtColor, Print, ResetColor, SetBackgroundColor, SetForegroundColor,
};
use crossterm::terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode, size,
};

use termray::{
    Camera, Color, FloorTexturer, Framebuffer, GridMap, HeightMap, HitSide, TILE_EMPTY, TILE_WALL,
    TileMap, TileType, WallTexturer, render_floor_ceiling, render_walls_with_heights,
};

const MAP_W: usize = 12;
const MAP_H: usize = 12;

/// Per-tile heights. `floor_rows` / `ceiling_rows` are flattened 2D arrays
/// in row-major order (y * MAP_W + x). Out-of-bounds queries fall back to
/// 0.0 / 1.0, matching the trait's recommended sane defaults.
struct TerrainHeights {
    floor_rows: Vec<f64>,
    ceiling_rows: Vec<f64>,
}

impl TerrainHeights {
    fn new() -> Self {
        let mut floor_rows = vec![0.0_f64; MAP_W * MAP_H];
        let mut ceiling_rows = vec![1.0_f64; MAP_W * MAP_H];

        // Tall tower — ceiling reaches up to 1.6 (normal walls end at 1.0).
        for (x, y) in [(4, 4), (4, 5), (5, 4), (5, 5)] {
            ceiling_rows[y * MAP_W + x] = 1.6;
        }
        // Low fence row (ceiling only 0.4 so you can peek over it).
        for x in 7..10 {
            ceiling_rows[3 * MAP_W + x] = 0.4;
        }
        // Sunken pit — floor drops to -0.3, ceiling still 1.0. Looks like
        // a step you can descend into; walls around it will grow taller.
        for (x, y) in [(8, 7), (8, 8), (9, 7), (9, 8)] {
            floor_rows[y * MAP_W + x] = -0.3;
        }
        // Raised plateau — floor at 0.3, ceiling lifted to 1.3 so the
        // player's head doesn't scrape when standing on it.
        for (x, y) in [(2, 8), (2, 9), (3, 8), (3, 9)] {
            floor_rows[y * MAP_W + x] = 0.3;
            ceiling_rows[y * MAP_W + x] = 1.3;
        }

        Self {
            floor_rows,
            ceiling_rows,
        }
    }
}

impl HeightMap for TerrainHeights {
    fn floor_height(&self, x: i32, y: i32) -> f64 {
        if (0..MAP_W as i32).contains(&x) && (0..MAP_H as i32).contains(&y) {
            self.floor_rows[y as usize * MAP_W + x as usize]
        } else {
            0.0
        }
    }

    fn ceiling_height(&self, x: i32, y: i32) -> f64 {
        if (0..MAP_W as i32).contains(&x) && (0..MAP_H as i32).contains(&y) {
            self.ceiling_rows[y as usize * MAP_W + x as usize]
        } else {
            1.0
        }
    }
}

struct SolidTexturer;

impl WallTexturer for SolidTexturer {
    fn sample_wall(
        &self,
        _tile: TileType,
        _wall_x: f64,
        wall_y: f64,
        side: HitSide,
        brightness: f64,
        _tile_hash: u32,
    ) -> Color {
        // Shade the wall top-to-bottom so stepped heights are obvious at a
        // glance: higher up = cooler, nearer the floor = warmer.
        let base = match side {
            HitSide::Vertical => Color::rgb(180, 200, 220),
            HitSide::Horizontal => Color::rgb(140, 160, 190),
        };
        let warm = Color::rgb(220, 180, 140);
        let t = wall_y.clamp(0.0, 1.0);
        let r = (base.r as f64 * (1.0 - t) + warm.r as f64 * t) as u8;
        let g = (base.g as f64 * (1.0 - t) + warm.g as f64 * t) as u8;
        let b = (base.b as f64 * (1.0 - t) + warm.b as f64 * t) as u8;
        Color::rgb(r, g, b).darken(brightness)
    }
}

impl FloorTexturer for SolidTexturer {
    fn sample_floor(&self, wx: f64, wy: f64, brightness: f64) -> Color {
        let on_grid = wx.fract().abs() < 0.04
            || wx.fract().abs() > 0.96
            || wy.fract().abs() < 0.04
            || wy.fract().abs() > 0.96;
        let base = if on_grid {
            Color::rgb(50, 55, 65)
        } else {
            Color::rgb(90, 95, 110)
        };
        base.darken(brightness)
    }

    fn sample_ceiling(&self, _wx: f64, _wy: f64, brightness: f64) -> Color {
        Color::rgb(40, 45, 60).darken(brightness)
    }
}

fn build_map() -> GridMap {
    let mut map = GridMap::new(MAP_W, MAP_H);
    for x in 1..MAP_W - 1 {
        for y in 1..MAP_H - 1 {
            map.set(x, y, TILE_EMPTY);
        }
    }
    // Central tower — four solid tiles in a 2x2 block.
    for (x, y) in [(4, 4), (4, 5), (5, 4), (5, 5)] {
        map.set(x, y, TILE_WALL);
    }
    // Low fence — a short run of solid tiles that are only 0.4 tall.
    for x in 7..10 {
        map.set(x, 3, TILE_WALL);
    }
    map
}

fn present(fb: &Framebuffer, status: &str) -> std::io::Result<()> {
    let mut stdout = stdout();
    execute!(stdout, MoveTo(0, 0))?;
    for y in (0..fb.height()).step_by(2) {
        for x in 0..fb.width() {
            let top = fb.get_pixel(x, y);
            let bot = fb.get_pixel(x, (y + 1).min(fb.height() - 1));
            execute!(
                stdout,
                SetForegroundColor(CtColor::Rgb {
                    r: top.r,
                    g: top.g,
                    b: top.b
                }),
                SetBackgroundColor(CtColor::Rgb {
                    r: bot.r,
                    g: bot.g,
                    b: bot.b
                }),
                Print("▀"),
            )?;
        }
        execute!(stdout, ResetColor, Print("\r\n"))?;
    }
    execute!(stdout, ResetColor, Print(status))?;
    stdout.flush()
}

fn step_x(map: &GridMap, x: f64, y: f64, dx: f64, radius: f64) -> (f64, bool) {
    if dx == 0.0 {
        return (x, false);
    }
    let nx = x + dx;
    let probe_x = nx + dx.signum() * radius;
    if map.is_solid(probe_x.floor() as i32, y.floor() as i32) {
        (x, true)
    } else {
        (nx, false)
    }
}

fn step_y(map: &GridMap, x: f64, y: f64, dy: f64, radius: f64) -> (f64, bool) {
    if dy == 0.0 {
        return (y, false);
    }
    let ny = y + dy;
    let probe_y = ny + dy.signum() * radius;
    if map.is_solid(x.floor() as i32, probe_y.floor() as i32) {
        (y, true)
    } else {
        (ny, false)
    }
}

fn main() -> std::io::Result<()> {
    let (cols, rows) = size()?;
    let fb_w = cols as usize;
    let fb_h = (rows as usize).saturating_sub(2) * 2;

    let map = build_map();
    let heights = TerrainHeights::new();
    let tex = SolidTexturer;

    // Start in the open area. Eye height will be adjusted every frame to
    // (floor_height + 0.5), so stepping onto the plateau lifts the camera.
    let mut cam = Camera::with_z(2.0, 2.0, 0.5, 0.0, 70f64.to_radians());
    let mut fb = Framebuffer::new(fb_w, fb_h);

    let mut vx = 0.0_f64;
    let mut vy = 0.0_f64;
    let accel = 12.0;
    let friction = 6.0;
    let turn_rate = 2.4;
    let radius = 0.2;

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, Hide, Clear(ClearType::All))?;

    let mut last = Instant::now();
    let mut running = true;

    while running {
        let now = Instant::now();
        let dt = (now - last).as_secs_f64().min(0.05);
        last = now;

        let mut dvx = 0.0;
        let mut dvy = 0.0;
        let mut dyaw = 0.0;
        while poll(Duration::from_millis(0))? {
            if let Event::Key(key) = read()? {
                let fwd = cam.forward();
                let right = cam.right();
                match key.code {
                    KeyCode::Esc => {
                        running = false;
                        break;
                    }
                    KeyCode::Char('w') => {
                        dvx += fwd.x * accel * dt;
                        dvy += fwd.y * accel * dt;
                    }
                    KeyCode::Char('s') => {
                        dvx -= fwd.x * accel * dt;
                        dvy -= fwd.y * accel * dt;
                    }
                    KeyCode::Char('d') => {
                        dvx += right.x * accel * dt;
                        dvy += right.y * accel * dt;
                    }
                    KeyCode::Char('a') => {
                        dvx -= right.x * accel * dt;
                        dvy -= right.y * accel * dt;
                    }
                    KeyCode::Char('q') => dyaw -= turn_rate * dt,
                    KeyCode::Char('e') => dyaw += turn_rate * dt,
                    _ => {}
                }
            }
        }

        vx += dvx;
        vy += dvy;
        let decay = (-friction * dt).exp();
        vx *= decay;
        vy *= decay;

        let (nx, blocked_x) = step_x(&map, cam.x, cam.y, vx * dt, radius);
        let (ny, blocked_y) = step_y(&map, nx, cam.y, vy * dt, radius);
        if blocked_x {
            vx = 0.0;
        }
        if blocked_y {
            vy = 0.0;
        }

        // Track the floor beneath the player so the camera rises when
        // stepping onto the raised plateau and sinks into the pit.
        let tile_x = nx.floor() as i32;
        let tile_y = ny.floor() as i32;
        let floor_h = heights.floor_height(tile_x, tile_y);
        let new_z = floor_h + 0.5;

        let new_yaw = cam.angle + dyaw;
        cam.set_pose_z(nx, ny, new_z, new_yaw);

        fb.clear(Color::default());
        let rays = cam.cast_all_rays(&map, fb_w, 16.0);
        // Phase 1 limitation: floor/ceiling still use the flat-plane renderer.
        render_floor_ceiling(&mut fb, &rays, &tex, &cam);
        render_walls_with_heights(&mut fb, &rays, &tex, &heights, &cam, 16.0);

        let yaw_deg = cam.angle.to_degrees().rem_euclid(360.0);
        let status = format!(
            "x={:.2} y={:.2} z={:.2} yaw={:5.1}°  [wasd=strafe qe=turn esc=quit]",
            cam.x, cam.y, cam.z, yaw_deg
        );
        present(&fb, &status)?;

        std::thread::sleep(Duration::from_millis(16));
    }

    execute!(stdout(), Show, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
