#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn darken(self, factor: f64) -> Self {
        let f = factor.clamp(0.0, 1.0);
        Self {
            r: (self.r as f64 * f) as u8,
            g: (self.g as f64 * f) as u8,
            b: (self.b as f64 * f) as u8,
        }
    }
}

pub struct Framebuffer {
    width: usize,
    height: usize,
    pixels: Vec<Color>,
}

impl Framebuffer {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![Color::default(); width * height],
        }
    }

    pub fn clear(&mut self, color: Color) {
        self.pixels.fill(color);
    }

    pub fn set_pixel(&mut self, x: usize, y: usize, color: Color) {
        if x < self.width && y < self.height {
            self.pixels[y * self.width + x] = color;
        }
    }

    /// Alpha-blend `color` over the existing pixel. `alpha` is 0.0 (invisible) to 1.0 (opaque).
    pub fn blend_pixel(&mut self, x: usize, y: usize, color: Color, alpha: f64) {
        if x < self.width && y < self.height {
            let alpha = alpha.clamp(0.0, 1.0);
            let bg = self.pixels[y * self.width + x];
            let inv = 1.0 - alpha;
            let r = (color.r as f64 * alpha + bg.r as f64 * inv) as u8;
            let g = (color.g as f64 * alpha + bg.g as f64 * inv) as u8;
            let b = (color.b as f64 * alpha + bg.b as f64 * inv) as u8;
            self.pixels[y * self.width + x] = Color::rgb(r, g, b);
        }
    }

    pub fn get_pixel(&self, x: usize, y: usize) -> Color {
        if x < self.width && y < self.height {
            self.pixels[y * self.width + x]
        } else {
            Color::default()
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Darken every pixel by `factor` (1.0 = unchanged, 0.0 = black).
    pub fn darken_all(&mut self, factor: f64) {
        for pixel in &mut self.pixels {
            *pixel = pixel.darken(factor);
        }
    }

    /// Shift all rows down by `amount` pixels, filling the top with black.
    pub fn shift_down(&mut self, amount: usize) {
        if amount == 0 || amount >= self.height {
            return;
        }
        for y in (amount..self.height).rev() {
            let src = (y - amount) * self.width;
            let dst = y * self.width;
            self.pixels.copy_within(src..src + self.width, dst);
        }
        let fill_end = amount * self.width;
        self.pixels[..fill_end].fill(Color::default());
    }
}
