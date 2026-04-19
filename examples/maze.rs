//! Minimal interactive demo: walk around a tiny maze rendered in the terminal.
//!
//! Run with `cargo run --example maze`. Controls: `w`/`a`/`s`/`d` to move and turn,
//! `q` to quit. Uses half-block characters + 24-bit truecolor to get ~double
//! vertical resolution.

use std::io::{Write, stdout};
use std::time::Duration;

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
    TILE_WALL, TileType, WallTexturer, render_floor_ceiling, render_walls,
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
            HitSide::Vertical => Color::rgb(200, 170, 140),
            HitSide::Horizontal => Color::rgb(170, 140, 110),
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
            Color::rgb(70, 60, 50)
        } else {
            Color::rgb(110, 95, 75)
        };
        base.darken(brightness)
    }

    fn sample_ceiling(&self, _wx: f64, _wy: f64, brightness: f64) -> Color {
        Color::rgb(60, 70, 90).darken(brightness)
    }
}

fn build_map() -> GridMap {
    // 10×10 box with a few interior walls
    const W: usize = 10;
    const H: usize = 10;
    let mut map = GridMap::new(W, H);
    for x in 1..W - 1 {
        for y in 1..H - 1 {
            map.set(x, y, TILE_EMPTY);
        }
    }
    // Interior obstacles
    map.set(3, 3, TILE_WALL);
    map.set(3, 4, TILE_WALL);
    map.set(4, 3, TILE_WALL);
    map.set(6, 6, TILE_WALL);
    map.set(6, 5, TILE_WALL);
    map.set(7, 6, TILE_WALL);
    map
}

fn present(fb: &Framebuffer) -> std::io::Result<()> {
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
    stdout.flush()
}

fn main() -> std::io::Result<()> {
    let (cols, rows) = size()?;
    let fb_w = cols as usize;
    let fb_h = (rows as usize).saturating_sub(1) * 2; // half-blocks double vertical res

    let map = build_map();
    let tex = SolidTexturer;

    let mut cam = Camera::new(5.0, 5.0, 0.0, 70f64.to_radians());
    let mut fb = Framebuffer::new(fb_w, fb_h);

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, Hide, Clear(ClearType::All))?;

    loop {
        fb.clear(Color::default());
        let rays = cam.cast_all_rays(&map, fb_w, 16.0);
        render_floor_ceiling(&mut fb, &rays, &tex, &FlatHeightMap, &cam, 16.0);
        render_walls(&mut fb, &rays, &tex, &FlatHeightMap, &cam, 16.0);
        present(&fb)?;

        if poll(Duration::from_millis(16))? {
            if let Event::Key(key) = read()? {
                let step = 0.2;
                let turn = 0.08;
                let (dx, dy) = (cam.angle.cos() * step, cam.angle.sin() * step);
                let mut nx = cam.x;
                let mut ny = cam.y;
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('w') => {
                        nx += dx;
                        ny += dy;
                    }
                    KeyCode::Char('s') => {
                        nx -= dx;
                        ny -= dy;
                    }
                    KeyCode::Char('a') => cam.angle -= turn,
                    KeyCode::Char('d') => cam.angle += turn,
                    _ => {}
                }
                // Minimal collision: only move if target cell is empty.
                if let Some(TILE_EMPTY) =
                    termray::TileMap::get(&map, nx.floor() as i32, ny.floor() as i32)
                {
                    cam.x = nx;
                    cam.y = ny;
                }
            }
        }
    }

    execute!(stdout(), Show, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
