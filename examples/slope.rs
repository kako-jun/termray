//! Continuous-slope demo for termray v0.3.0.
//!
//! A 20×16 open arena with two smooth features driven by per-corner floor
//! heights — a rounded hill centered around (8, 8) and a bowl-shaped valley
//! around (14, 4). Unlike `examples/terrain.rs` (which paints per-tile steps)
//! this one lets adjacent cells share non-equal corner heights, so the ground
//! plane renders as a continuous slope through the bilinear-interpolated
//! `CornerHeights`.
//!
//! Run with `cargo run --example slope`. Controls:
//! - `w` / `s` — accelerate forward / back along the camera's facing
//! - `a` / `d` — strafe left / right (perpendicular to facing)
//! - `q` / `e` — turn left / right
//! - `r` / `f` — pitch up / down (looking above / below the horizon)
//! - `esc` — quit

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
    Camera, Color, CornerHeights, FloorTexturer, Framebuffer, GridMap, HeightMap, HitSide,
    TILE_EMPTY, TileMap, TileType, WallTexturer, render_floor_ceiling, render_walls,
};

const MAP_W: usize = 20;
const MAP_H: usize = 16;

/// Continuous height field sampled at world-corner resolution.
///
/// `height_at_corner(cx, cy)` is the floor elevation at world corner
/// `(cx, cy)`. `cell_heights` gathers the four corners of a cell, which
/// guarantees the continuity contract: `here.NE == east.NW`, etc.
struct Landscape;

impl Landscape {
    fn height_at_corner(cx: i32, cy: i32) -> f64 {
        let fx = cx as f64;
        let fy = cy as f64;
        // Smooth hill centered at (8, 8), radius ~5 — max height 0.45.
        let hill = {
            let dx = fx - 8.0;
            let dy = fy - 8.0;
            let r2 = (dx * dx + dy * dy) / (5.0 * 5.0);
            (0.45 * (-r2 * 1.2).exp()).max(0.0)
        };
        // Bowl around (14, 4), digging down to -0.35 in the middle.
        let bowl = {
            let dx = fx - 14.0;
            let dy = fy - 4.0;
            let r2 = (dx * dx + dy * dy) / (3.5 * 3.5);
            -(0.35 * (-r2 * 1.4).exp())
        };
        hill + bowl
    }
}

impl HeightMap for Landscape {
    fn cell_heights(&self, x: i32, y: i32) -> CornerHeights {
        CornerHeights {
            floor: [
                Self::height_at_corner(x, y),         // NW
                Self::height_at_corner(x + 1, y),     // NE
                Self::height_at_corner(x, y + 1),     // SW
                Self::height_at_corner(x + 1, y + 1), // SE
            ],
            ceil: [1.0; 4],
        }
    }
}

struct Scene;

impl WallTexturer for Scene {
    fn sample_wall(
        &self,
        _tile: TileType,
        _wall_x: f64,
        wall_y: f64,
        side: HitSide,
        brightness: f64,
        _tile_hash: u32,
    ) -> Color {
        let base = match side {
            HitSide::Vertical => Color::rgb(180, 160, 140),
            HitSide::Horizontal => Color::rgb(140, 130, 120),
        };
        // Subtle vertical gradient so corner-interpolated walls read clearly.
        let t = wall_y.clamp(0.0, 1.0);
        let warm = Color::rgb(200, 150, 110);
        let r = (base.r as f64 * (1.0 - t) + warm.r as f64 * t) as u8;
        let g = (base.g as f64 * (1.0 - t) + warm.g as f64 * t) as u8;
        let b = (base.b as f64 * (1.0 - t) + warm.b as f64 * t) as u8;
        Color::rgb(r, g, b).darken(brightness)
    }
}

impl FloorTexturer for Scene {
    fn sample_floor(&self, wx: f64, wy: f64, brightness: f64) -> Color {
        // Fine grid so the slope distortion is easy to read.
        let on_grid = wx.fract().abs() < 0.03
            || wx.fract().abs() > 0.97
            || wy.fract().abs() < 0.03
            || wy.fract().abs() > 0.97;
        let base = if on_grid {
            Color::rgb(60, 80, 50)
        } else {
            Color::rgb(90, 125, 80)
        };
        base.darken(brightness)
    }

    fn sample_ceiling(&self, _wx: f64, _wy: f64, brightness: f64) -> Color {
        Color::rgb(110, 150, 190).darken(brightness)
    }
}

fn build_map() -> GridMap {
    let mut map = GridMap::new(MAP_W, MAP_H);
    for x in 1..MAP_W - 1 {
        for y in 1..MAP_H - 1 {
            map.set(x, y, TILE_EMPTY);
        }
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
    let heights = Landscape;
    let scene = Scene;

    let mut cam = Camera::with_z(3.0, 8.0, 0.5, 0.0, 70f64.to_radians());
    let mut fb = Framebuffer::new(fb_w, fb_h);

    let mut vx = 0.0_f64;
    let mut vy = 0.0_f64;
    let accel = 12.0;
    let friction = 6.0;
    let turn_rate = 2.4;
    let pitch_rate = 1.5;
    let pitch_limit = 1.2; // keep under FRAC_PI_2 to avoid tan singularity
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
        let mut dpitch = 0.0;
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
                    KeyCode::Char('r') => dpitch += pitch_rate * dt,
                    KeyCode::Char('f') => dpitch -= pitch_rate * dt,
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

        // Eye height tracks the slope under the player. Bilinear-sample
        // the floor at the exact (nx, ny) so smooth terrain gives smooth
        // vertical motion, not stair-step jumps.
        let cx = nx.floor() as i32;
        let cy = ny.floor() as i32;
        let u = nx - cx as f64;
        let v = ny - cy as f64;
        let floor_h = heights.cell_heights(cx, cy).sample_floor(u, v);
        let new_z = floor_h + 0.5;

        let new_yaw = cam.angle + dyaw;
        let new_pitch = (cam.pitch + dpitch).clamp(-pitch_limit, pitch_limit);
        cam.set_pose(nx, ny, new_yaw);
        cam.set_z(new_z);
        cam.set_pitch(new_pitch);

        fb.clear(Color::default());
        let rays = cam.cast_all_rays(&map, fb_w, 18.0);
        render_floor_ceiling(&mut fb, &rays, &scene, &heights, &cam, 18.0);
        render_walls(&mut fb, &rays, &scene, &heights, &cam, 18.0);

        let yaw_deg = cam.angle.to_degrees().rem_euclid(360.0);
        let pitch_deg = cam.pitch.to_degrees();
        let status = format!(
            "x={:.2} y={:.2} z={:.2} yaw={:5.1}° pitch={:+5.1}°  [wasd qe / rf=pitch esc=quit]",
            cam.x, cam.y, cam.z, yaw_deg, pitch_deg
        );
        present(&fb, &status)?;

        std::thread::sleep(Duration::from_millis(16));
    }

    execute!(stdout(), Show, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
