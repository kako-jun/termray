//! Integration tests for the `label` module.

use termray::{
    project_labels, render_labels, Color, Font8x8, Framebuffer, GlyphRenderer, HitSide, Label,
    RayHit,
};

#[test]
fn project_excludes_labels_behind_camera() {
    let fov = 70f64.to_radians();
    // Camera at origin facing +x (angle 0). A label behind us at (-5, 0) should be culled.
    let labels = vec![Label {
        text: "behind".into(),
        x: -5.0,
        y: 0.0,
        world_height: 0.8,
        color: Color::rgb(255, 255, 255),
        background: None,
        max_chars: None,
    }];
    let projected = project_labels(&labels, 0.0, 0.0, 0.0, fov, 80);
    assert!(projected.is_empty(), "label behind camera should be culled");

    // Sanity: a label in front should survive.
    let front = vec![Label {
        text: "front".into(),
        x: 5.0,
        y: 0.0,
        world_height: 0.8,
        color: Color::rgb(255, 255, 255),
        background: None,
        max_chars: None,
    }];
    let projected = project_labels(&front, 0.0, 0.0, 0.0, fov, 80);
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].lines, vec!["front".to_string()]);
}

#[test]
fn wrap_handles_oversize_word() {
    let fov = 70f64.to_radians();
    let labels = vec![Label {
        text: "supercalifragilistic".into(),
        x: 3.0,
        y: 0.0,
        world_height: 0.8,
        color: Color::rgb(255, 255, 255),
        background: None,
        max_chars: Some(6),
    }];
    let p = project_labels(&labels, 0.0, 0.0, 0.0, fov, 80);
    assert_eq!(p.len(), 1);
    // "supercalifragilistic" (20 chars) split at 6 -> "superc", "alifra", "gilist", "ic"
    assert_eq!(
        p[0].lines,
        vec!["superc", "alifra", "gilist", "ic"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    );
}

#[test]
fn wrap_empty_text_drops_label() {
    let fov = 70f64.to_radians();
    let labels = vec![Label {
        text: "".into(),
        x: 3.0,
        y: 0.0,
        world_height: 0.8,
        color: Color::rgb(255, 255, 255),
        background: None,
        max_chars: Some(8),
    }];
    let p = project_labels(&labels, 0.0, 0.0, 0.0, fov, 80);
    assert!(p.is_empty());
}

#[test]
fn wrap_greedy_final_word_just_fits() {
    let fov = 70f64.to_radians();
    let labels = vec![Label {
        text: "hi there longword".into(),
        x: 3.0,
        y: 0.0,
        world_height: 0.8,
        color: Color::rgb(255, 255, 255),
        background: None,
        max_chars: Some(8),
    }];
    let p = project_labels(&labels, 0.0, 0.0, 0.0, fov, 80);
    assert_eq!(p.len(), 1);
    assert_eq!(
        p[0].lines,
        vec!["hi there", "longword"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    );
    // sanity: distance should be exactly 3.0.
    assert!((p[0].distance - 3.0).abs() < 1e-9);
}

#[test]
fn font8x8_draws_at_least_one_pixel_for_ascii_upper_a() {
    let mut fb = Framebuffer::new(16, 16);
    let white = Color::rgb(255, 255, 255);
    Font8x8.draw_glyph(&mut fb, 0, 0, 'A', white);
    let mut count = 0usize;
    for y in 0..16 {
        for x in 0..16 {
            if fb.get_pixel(x, y) == white {
                count += 1;
            }
        }
    }
    assert!(count > 0, "Font8x8 should draw at least one pixel for 'A'");
}

#[test]
fn font8x8_ignores_chars_outside_basic_latin() {
    let mut fb = Framebuffer::new(16, 16);
    let white = Color::rgb(255, 255, 255);
    // U+00A0 (nbsp) is outside basic_latin range 0x20..=0x7E.
    Font8x8.draw_glyph(&mut fb, 0, 0, '\u{00A0}', white);
    // Outside the range -> nothing drawn.
    for y in 0..16 {
        for x in 0..16 {
            assert_eq!(fb.get_pixel(x, y), Color::default());
        }
    }
}

#[test]
fn render_skips_labels_occluded_by_walls() {
    // Build a framebuffer and a ray map where every column reports a wall
    // at distance 1.0. A label at distance 5.0 must not appear anywhere.
    let w = 80usize;
    let h = 40usize;
    let mut fb = Framebuffer::new(w, h);
    // Pre-fill with a sentinel so we can detect any pixel write.
    let bg_sentinel = Color::rgb(1, 2, 3);
    fb.clear(bg_sentinel);

    let rays: Vec<Option<RayHit>> = (0..w)
        .map(|i| {
            Some(RayHit {
                distance: 1.0,
                side: if i % 2 == 0 {
                    HitSide::Vertical
                } else {
                    HitSide::Horizontal
                },
                map_x: 0,
                map_y: 0,
                wall_x: 0.0,
                tile: 1,
            })
        })
        .collect();

    let fov = 70f64.to_radians();
    let labels = vec![Label {
        text: "HIDDEN".into(),
        x: 5.0,
        y: 0.0,
        world_height: 0.8,
        color: Color::rgb(255, 255, 255),
        background: Some(Color::rgb(10, 10, 10)),
        max_chars: None,
    }];
    let projected = project_labels(&labels, 0.0, 0.0, 0.0, fov, w);
    assert_eq!(projected.len(), 1);
    assert!(projected[0].distance > 1.0);

    render_labels(&mut fb, &projected, &rays, &Font8x8, 16.0);

    // No pixel should have been touched.
    for y in 0..h {
        for x in 0..w {
            assert_eq!(
                fb.get_pixel(x, y),
                bg_sentinel,
                "pixel ({x},{y}) was written despite full wall occlusion"
            );
        }
    }
}

#[test]
fn render_draws_label_when_unobstructed() {
    let w = 80usize;
    let h = 40usize;
    let mut fb = Framebuffer::new(w, h);
    let bg_sentinel = Color::rgb(1, 2, 3);
    fb.clear(bg_sentinel);

    // No walls in any column.
    let rays: Vec<Option<RayHit>> = (0..w).map(|_| None).collect();

    let fov = 70f64.to_radians();
    let labels = vec![Label {
        text: "HI".into(),
        // Close enough that the projected baseline lands on-screen.
        x: 3.0,
        y: 0.0,
        world_height: 0.2,
        color: Color::rgb(255, 255, 255),
        background: None,
        max_chars: None,
    }];
    let projected = project_labels(&labels, 0.0, 0.0, 0.0, fov, w);
    assert_eq!(projected.len(), 1);

    render_labels(&mut fb, &projected, &rays, &Font8x8, 16.0);

    // At least one pixel should differ from the sentinel.
    let mut touched = 0usize;
    for y in 0..h {
        for x in 0..w {
            if fb.get_pixel(x, y) != bg_sentinel {
                touched += 1;
            }
        }
    }
    assert!(touched > 0, "unobstructed label should draw some pixels");
}
