//! Physics-style free camera demo.
//!
//! Unlike `maze.rs` (which moves the camera directly on each keystroke),
//! this example keeps the camera's pose out of the input handler entirely.
//! Input only mutates a velocity vector; a tiny Euler integrator with friction
//! and wall clamping runs every frame and then feeds the new pose in via
//! [`termray::Camera::set_pose`]. Swap the integrator for `rapier3d` (or any
//! other physics backend) and the rendering side does not change.
//!
//! Run with `cargo run --example free_camera`. Controls:
//! - `w` / `s` — accelerate forward / back along the camera's facing
//! - `a` / `d` — strafe left / right (perpendicular to facing)
//! - `q` / `e` — turn left / right
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
    Camera, Color, FlatHeightMap, FloorTexturer, Framebuffer, GridMap, HitSide, TILE_EMPTY,
    TILE_WALL, TileMap, TileType, WallTexturer, render_floor_ceiling, render_walls,
};

struct SolidTexturer;

impl WallTexturer for SolidTexturer {
    fn sample_wall(
        &self,
        _tile: TileType,
        _wall_x: f64,
        _wall_y: f64,
        side: HitSide,
        brightness: f64,
        _tile_hash: u32,
    ) -> Color {
        let base = match side {
            HitSide::Vertical => Color::rgb(180, 190, 210),
            HitSide::Horizontal => Color::rgb(140, 150, 170),
        };
        base.darken(brightness)
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
    const W: usize = 12;
    const H: usize = 12;
    let mut map = GridMap::new(W, H);
    for x in 1..W - 1 {
        for y in 1..H - 1 {
            map.set(x, y, TILE_EMPTY);
        }
    }
    // Scattered pillars so strafing / physics feels meaningful.
    for (x, y) in [(3, 3), (3, 8), (8, 3), (8, 8), (5, 5), (6, 6)] {
        map.set(x, y, TILE_WALL);
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

/// Axis-separated slide collision — move along X only and clamp at solid
/// tiles. `radius` is the body's half-extent along the movement axis so the
/// camera stops a little before actually entering a wall cell. Returns the
/// new X along with whether the step was blocked (so the caller can zero out
/// that axis's velocity).
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

/// Y counterpart of [`step_x`]. Called after `step_x` has already committed
/// to a new X, giving standard axis-separated slide behavior (the player
/// slides along walls instead of stopping dead).
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
    let tex = SolidTexturer;

    let mut cam = Camera::new(6.0, 6.0, 0.0, 70f64.to_radians());
    let mut fb = Framebuffer::new(fb_w, fb_h);

    // Physics state lives outside the camera. The camera only receives poses
    // via `set_pose`, mirroring how a rapier3d rigid body would drive it.
    let mut vx = 0.0_f64;
    let mut vy = 0.0_f64;
    let accel = 12.0; // world units / sec²
    let friction = 6.0; // velocity decay per sec
    let turn_rate = 2.4; // rad / sec while held
    let radius = 0.2;

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, Hide, Clear(ClearType::All))?;

    let mut last = Instant::now();
    let mut running = true;

    while running {
        let now = Instant::now();
        let dt = (now - last).as_secs_f64().min(0.05);
        last = now;

        // Drain all pending events this frame to get responsive input.
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

        // Integrate: accumulate input impulses, apply friction, clamp at walls.
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

        let new_yaw = cam.angle + dyaw;
        // Hand the freshly integrated pose to the camera in one call —
        // exactly the seam a physics engine would use.
        cam.set_pose(nx, ny, new_yaw);

        fb.clear(Color::default());
        let rays = cam.cast_all_rays(&map, fb_w, 16.0);
        render_floor_ceiling(&mut fb, &rays, &tex, &FlatHeightMap, &cam, 16.0);
        render_walls(&mut fb, &rays, &tex, &FlatHeightMap, &cam, 16.0);

        let yaw_deg = cam.angle.to_degrees().rem_euclid(360.0);
        let status = format!(
            "x={:.2} y={:.2} yaw={:5.1}° vx={:+.2} vy={:+.2}  [wasd=strafe qe=turn esc=quit]",
            cam.x, cam.y, yaw_deg, vx, vy
        );
        present(&fb, &status)?;

        // Cap update rate roughly; we still sleep a tiny bit so we don't spin.
        std::thread::sleep(Duration::from_millis(16));
    }

    execute!(stdout(), Show, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
