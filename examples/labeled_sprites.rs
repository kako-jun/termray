//! Showcase for termray #5 — world-anchored text labels.
//!
//! A friendly-filer–style arrangement: a small room with a handful of file
//! "icons" (sprites) each carrying a caption (the file name). Labels are
//! drawn at fixed pixel size so names stay readable near and far, and the
//! per-column depth test means captions correctly disappear when you walk
//! behind a wall.
//!
//! Run with `cargo run --example labeled_sprites`. Controls:
//! - `w` / `s` — walk forward / back
//! - `a` / `d` — turn left / right
//! - `q`       — quit

use std::io::{stdout, Write};
use std::time::Duration;

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{poll, read, Event, KeyCode};
use crossterm::execute;
use crossterm::style::{
    Color as CtColor, Print, ResetColor, SetBackgroundColor, SetForegroundColor,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, size, Clear, ClearType, EnterAlternateScreen,
    LeaveAlternateScreen,
};

use termray::{
    project_labels, project_sprites, render_floor_ceiling, render_labels, render_sprites,
    render_walls, Camera, Color, FloorTexturer, Font8x8, Framebuffer, GridMap, HitSide, Label,
    Sprite, SpriteArt, SpriteDef, TileType, WallTexturer, TILE_EMPTY, TILE_WALL,
};

// ---------- world / textures ----------

struct Scene;

impl WallTexturer for Scene {
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
            HitSide::Vertical => Color::rgb(190, 180, 160),
            HitSide::Horizontal => Color::rgb(155, 145, 125),
        };
        base.darken(brightness)
    }
}

impl FloorTexturer for Scene {
    fn sample_floor(&self, wx: f64, wy: f64, brightness: f64) -> Color {
        let on_grid = wx.fract().abs() < 0.04
            || wx.fract().abs() > 0.96
            || wy.fract().abs() < 0.04
            || wy.fract().abs() > 0.96;
        let base = if on_grid {
            Color::rgb(60, 55, 45)
        } else {
            Color::rgb(100, 90, 70)
        };
        base.darken(brightness)
    }

    fn sample_ceiling(&self, _wx: f64, _wy: f64, brightness: f64) -> Color {
        Color::rgb(50, 60, 80).darken(brightness)
    }
}

// ---------- sprite art (simple file-icon look) ----------

struct IconArt;

// A tiny "page with a corner fold" motif. Height/width ratio chosen so the
// result reads as a document icon.
const PAGE_ICON: SpriteDef = SpriteDef {
    pattern: &[
        "########", "#......#", "#......#", "#......#", "#......#", "#......#", "#......#",
        "########",
    ],
    height_scale: 0.45,
    float_offset_scale: 0.1,
};

// A "folder"-ish slab.
const FOLDER_ICON: SpriteDef = SpriteDef {
    pattern: &[
        "++######", "++++++++", "+......+", "+......+", "+......+", "+......+", "+......+",
        "++++++++",
    ],
    height_scale: 0.40,
    float_offset_scale: 0.05,
};

impl SpriteArt for IconArt {
    fn art(&self, sprite_type: u8) -> Option<&SpriteDef> {
        match sprite_type {
            0 => Some(&PAGE_ICON),
            1 => Some(&FOLDER_ICON),
            _ => None,
        }
    }

    fn color(&self, sprite_type: u8) -> Color {
        match sprite_type {
            0 => Color::rgb(230, 230, 240), // page
            1 => Color::rgb(220, 185, 120), // folder (manila)
            _ => Color::rgb(200, 200, 200),
        }
    }
}

// ---------- map ----------

fn build_map() -> GridMap {
    // 6 wide × 8 deep room. The player spawns near the south wall and faces north.
    const W: usize = 6;
    const H: usize = 8;
    let mut map = GridMap::new(W, H);
    for x in 1..W - 1 {
        for y in 1..H - 1 {
            map.set(x, y, TILE_EMPTY);
        }
    }
    // One interior wall the labels can hide behind.
    map.set(3, 4, TILE_WALL);
    map
}

fn build_sprites_and_labels() -> (Vec<Sprite>, Vec<Label>) {
    let white = Color::rgb(240, 240, 240);
    let soft_bg = Some(Color::rgb(20, 20, 25));

    // Each (sprite, label) pair sits at the same world position.
    let entries: &[(f64, f64, u8, &str)] = &[
        (2.0, 2.5, 0, "README.md"),
        (4.0, 2.5, 0, "notes.txt"),
        (2.0, 5.0, 1, "photos"),
        (4.0, 5.0, 0, "photo.jpg"),
        (2.5, 6.5, 0, "super long file name.rs"),
    ];

    let sprites = entries
        .iter()
        .map(|(x, y, t, _)| Sprite {
            x: *x,
            y: *y,
            sprite_type: *t,
        })
        .collect();

    let labels = entries
        .iter()
        .map(|(x, y, _, text)| Label {
            text: (*text).to_string(),
            x: *x,
            y: *y,
            // Lift the caption so it sits above the icon silhouette.
            world_height: 0.85,
            color: white,
            background: soft_bg,
            max_chars: Some(12),
        })
        .collect();

    (sprites, labels)
}

// ---------- present ----------

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
    let fb_h = (rows as usize).saturating_sub(1) * 2;

    let map = build_map();
    let scene = Scene;
    let art = IconArt;
    let (sprites, labels) = build_sprites_and_labels();

    // Spawn near the south wall, facing north (+y).
    let mut cam = Camera::new(3.0, 1.5, std::f64::consts::FRAC_PI_2, 70f64.to_radians());
    let mut fb = Framebuffer::new(fb_w, fb_h);

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, Hide, Clear(ClearType::All))?;

    loop {
        fb.clear(Color::default());
        let rays = cam.cast_all_rays(&map, fb_w, 16.0);
        render_floor_ceiling(&mut fb, &rays, &scene, &cam);
        render_walls(&mut fb, &rays, &scene, 16.0);

        let projected_sprites = project_sprites(&sprites, cam.x, cam.y, cam.angle, cam.fov, fb_w);
        render_sprites(&mut fb, &projected_sprites, &rays, &art, 16.0);

        let projected_labels = project_labels(&labels, cam.x, cam.y, cam.angle, cam.fov, fb_w);
        render_labels(&mut fb, &projected_labels, &rays, &Font8x8, 16.0);

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
