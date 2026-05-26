use anyhow::Result;
use fontdue::{Font, FontSettings};
use softbuffer::{Context, Surface};
use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::sync::Arc;
use winit::window::Window;

const N_BARS: usize = 75;

pub struct Overlay {
    pub window: Arc<Window>,
    _context: Context<Arc<Window>>,
    surface: Surface<Arc<Window>, Arc<Window>>,
    bars: VecDeque<f32>,
    recording: bool,
    frame: u32,
    font: Option<Font>,
}

fn load_font() -> Option<Font> {
    let settings = FontSettings { collection_index: 0, ..FontSettings::default() };
    let mut candidates: Vec<&str> = Vec::new();
    #[cfg(target_os = "macos")]
    candidates.extend_from_slice(&[
        "/System/Library/Fonts/Helvetica.ttc",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/Library/Fonts/Arial.ttf",
    ]);
    #[cfg(target_os = "windows")]
    candidates.extend_from_slice(&[
        r"C:\Windows\Fonts\segoeui.ttf",
        r"C:\Windows\Fonts\arial.ttf",
        r"C:\Windows\Fonts\calibri.ttf",
    ]);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    candidates.extend_from_slice(&[
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
    ]);
    for path in candidates {
        if let Ok(data) = std::fs::read(path) {
            if let Ok(font) = Font::from_bytes(data.as_slice(), settings) {
                return Some(font);
            }
        }
    }
    None
}

impl Overlay {
    pub fn new(window: Arc<Window>) -> Result<Self> {
        eprintln!("[overlay] creating softbuffer context...");
        let context = Context::new(window.clone())
            .map_err(|e| anyhow::anyhow!("softbuffer context: {e:?}"))?;
        eprintln!("[overlay] context ok, creating surface...");
        let surface = Surface::new(&context, window.clone())
            .map_err(|e| anyhow::anyhow!("softbuffer surface: {e:?}"))?;
        eprintln!("[overlay] surface ok");

        let mut bars = VecDeque::with_capacity(N_BARS + 1);
        for _ in 0..N_BARS {
            bars.push_back(0.0f32);
        }

        Ok(Self {
            window,
            _context: context,
            surface,
            bars,
            recording: false,
            frame: 0,
            font: load_font(),
        })
    }

    pub fn set_recording(&mut self, recording: bool) {
        self.recording = recording;
        if !recording {
            for b in &mut self.bars {
                *b = 0.0;
            }
        }
    }

    pub fn push_level(&mut self, level: f32) {
        self.bars.pop_front();
        self.bars.push_back(level.clamp(0.0, 1.0));
        self.frame = self.frame.wrapping_add(1);
    }

    pub fn draw(&mut self) -> Result<()> {
        let size = self.window.inner_size();
        let w = size.width;
        let h = size.height;

        self.surface
            .resize(
                NonZeroU32::new(w).unwrap_or(NonZeroU32::new(1).unwrap()),
                NonZeroU32::new(h).unwrap_or(NonZeroU32::new(1).unwrap()),
            )
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;

        let mut buffer = self.surface.buffer_mut().map_err(|e| anyhow::anyhow!("{e:?}"))?;
        draw_frame(
            &mut buffer,
            w as usize,
            h as usize,
            &self.bars,
            self.frame,
            self.font.as_ref(),
        );
        buffer.present().map_err(|e| anyhow::anyhow!("{e:?}"))?;
        Ok(())
    }
}

fn draw_frame(
    buf: &mut [u32],
    w: usize,
    h: usize,
    bars: &VecDeque<f32>,
    frame: u32,
    font: Option<&Font>,
) {
    // Skip draw if window hasn't been sized yet
    if w < 40 || h < 20 {
        return;
    }
    buf.fill(0x00000000);

    let pad = 5usize;
    let corner_r = 20usize;

    // White card background
    for y in 0..h {
        for x in 0..w {
            if in_rounded_rect(x, y, pad, pad, w - pad, h - pad, corner_r) {
                buf[y * w + x] = 0xFFF5F5F5;
            }
        }
    }

    // Subtle 1px border
    for y in 0..h {
        for x in 0..w {
            let inner = in_rounded_rect(x, y, pad + 1, pad + 1, w - pad - 1, h - pad - 1, corner_r.saturating_sub(1));
            let outer = in_rounded_rect(x, y, pad, pad, w - pad, h - pad, corner_r);
            if outer && !inner {
                buf[y * w + x] = 0xFFD1D1D6;
            }
        }
    }

    let cy = h / 2;

    // === Red pulsing dot ===
    let dot_r = 8usize;
    let dot_cx = pad + 22;
    let dot_cy = cy;
    let pulse = ((frame as f32 * 0.15).sin() * 0.25 + 0.75).clamp(0.0, 1.0);
    let dot_alpha = (210.0 + pulse * 45.0) as u32;
    let dot_rgb = 0x00FF3B30u32; // iOS system red

    for ry in dot_cy.saturating_sub(dot_r + 1)..=(dot_cy + dot_r + 1).min(h.saturating_sub(1)) {
        for rx in dot_cx.saturating_sub(dot_r + 1)..=(dot_cx + dot_r + 1).min(w.saturating_sub(1)) {
            let dx = rx as f32 - dot_cx as f32;
            let dy = ry as f32 - dot_cy as f32;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist <= dot_r as f32 {
                buf[ry * w + rx] = (dot_alpha << 24) | dot_rgb;
            } else if dist < dot_r as f32 + 1.0 {
                let aa = (1.0 - (dist - dot_r as f32)).clamp(0.0, 1.0);
                let a = ((dot_alpha as f32) * aa) as u32;
                buf[ry * w + rx] = blend_over((a << 24) | dot_rgb, buf[ry * w + rx]);
            }
        }
    }

    // === "Recording" label ===
    let text_x = dot_cx + dot_r + 9;
    if let Some(f) = font {
        draw_text(buf, w, h, f, "Recording", text_x, cy + 5, 13.5, 0xFF1C1C1E);
    }

    // === Waveform (centered, bidirectional bars) ===
    let wave_x0 = 140usize;
    let wave_x1 = w.saturating_sub(145);
    let max_half_h = h / 2 - pad - 6;

    if let (true, n) = (wave_x1 > wave_x0, bars.len()) {
        if n > 0 {
            let area_w = wave_x1 - wave_x0;
            let slot_w = (area_w / n).max(1);
            let bar_w = slot_w.max(1).min(2);

            for (i, &level) in bars.iter().enumerate() {
                let half_h = ((level * max_half_h as f32) as usize).max(2);
                let x0 = wave_x0 + i * slot_w;
                let y_top = cy.saturating_sub(half_h);
                let y_bot = (cy + half_h).min(h.saturating_sub(1));

                for py in y_top..=y_bot {
                    for px in x0..(x0 + bar_w).min(w) {
                        buf[py * w + px] = 0xFF1C1C1E;
                    }
                }
            }
        }
    }

    // === Right section: Stop / Cancel Esc ===
    if let Some(f) = font {
        let right_edge = w.saturating_sub(pad + 10);

        // "Esc" key badge
        let badge_h = 20usize;
        let badge_y = cy.saturating_sub(badge_h / 2);
        let esc_w = 28usize;
        let esc_x = right_edge.saturating_sub(esc_w);
        draw_badge(buf, w, h, esc_x, badge_y, esc_w, badge_h, 5);
        draw_text_in_badge(buf, w, h, f, "Esc", esc_x, badge_y, esc_w, badge_h, 10.0, 0xFF3C3C43);

        // "Cancel" text (right-aligned to Esc badge)
        let cancel_w = text_width(f, "Cancel", 13.5);
        let cancel_x = esc_x.saturating_sub(cancel_w + 7);
        draw_text(buf, w, h, f, "Cancel", cancel_x, cy + 5, 13.5, 0xFF1C1C1E);

        // Thin vertical divider
        let div_x = cancel_x.saturating_sub(10);
        for dy in cy.saturating_sub(10)..cy + 10 {
            if div_x < w {
                buf[dy * w + div_x] = blend_over(0x30000000, buf[dy * w + div_x]);
            }
        }

        // "Stop" text (right of divider)
        let stop_w = text_width(f, "Stop", 13.5);
        let stop_x = div_x.saturating_sub(stop_w + 7);
        draw_text(buf, w, h, f, "Stop", stop_x, cy + 5, 13.5, 0xFF1C1C1E);
    }
}

// ARGB blend: fg over opaque bg → opaque result
fn blend_over(fg: u32, bg: u32) -> u32 {
    let a = (fg >> 24) & 0xFF;
    if a == 0 { return bg; }
    if a == 255 { return fg; }
    let r = ((((fg >> 16) & 0xFF) * a + ((bg >> 16) & 0xFF) * (255 - a)) / 255) as u32;
    let g = ((((fg >> 8) & 0xFF) * a + ((bg >> 8) & 0xFF) * (255 - a)) / 255) as u32;
    let b = (((fg & 0xFF) * a + (bg & 0xFF) * (255 - a)) / 255) as u32;
    0xFF000000 | (r << 16) | (g << 8) | b
}

fn draw_badge(buf: &mut [u32], w: usize, h: usize, x: usize, y: usize, bw: usize, bh: usize, r: usize) {
    for py in y..(y + bh).min(h) {
        for px in x..(x + bw).min(w) {
            if in_rounded_rect(px, py, x, y, x + bw, y + bh, r) {
                buf[py * w + px] = 0xFFE5E5EA;
            }
        }
    }
}

fn draw_text(
    buf: &mut [u32],
    w: usize,
    h: usize,
    font: &Font,
    text: &str,
    x: usize,
    baseline_y: usize,
    size: f32,
    color: u32,
) -> usize {
    let mut cx = x as i32;
    let rgb = color & 0x00FFFFFF;
    for ch in text.chars() {
        let (metrics, bitmap) = font.rasterize(ch, size);
        let top_y = baseline_y as i32 - metrics.ymin - metrics.height as i32;
        for gy in 0..metrics.height {
            for gx in 0..metrics.width {
                let alpha = bitmap[gy * metrics.width + gx] as u32;
                if alpha == 0 { continue; }
                let px = cx + metrics.xmin + gx as i32;
                let py = top_y + gy as i32;
                if py >= 0 && (py as usize) < h && px >= 0 && (px as usize) < w {
                    let dest = &mut buf[py as usize * w + px as usize];
                    *dest = blend_over((alpha << 24) | rgb, *dest);
                }
            }
        }
        cx += metrics.advance_width.round() as i32;
    }
    cx as usize
}

fn draw_text_in_badge(
    buf: &mut [u32],
    w: usize,
    h: usize,
    font: &Font,
    text: &str,
    bx: usize,
    by: usize,
    bw: usize,
    bh: usize,
    size: f32,
    color: u32,
) {
    let tw = text_width(font, text, size);
    let cap_h = cap_height(font, size);
    let tx = bx + (bw.saturating_sub(tw)) / 2;
    let ty = by + (bh + cap_h) / 2;
    draw_text(buf, w, h, font, text, tx, ty, size, color);
}

fn text_width(font: &Font, text: &str, size: f32) -> usize {
    text.chars()
        .map(|c| font.metrics(c, size).advance_width.round() as usize)
        .sum()
}

fn cap_height(font: &Font, size: f32) -> usize {
    // 'H' has no descender; its height ≈ cap height
    font.metrics('H', size).height
}

fn in_rounded_rect(
    px: usize, py: usize,
    x0: usize, y0: usize,
    x1: usize, y1: usize,
    r: usize,
) -> bool {
    if px < x0 || px >= x1 || py < y0 || py >= y1 { return false; }
    let in_l = px < x0 + r;
    let in_r = x1 >= r && px >= x1 - r;
    let in_t = py < y0 + r;
    let in_b = y1 >= r && py >= y1 - r;
    let sq = |a: isize| a * a;
    if in_l && in_t {
        let dx = (x0 + r) as isize - px as isize;
        let dy = (y0 + r) as isize - py as isize;
        return sq(dx) + sq(dy) <= sq(r as isize);
    }
    if in_r && in_t {
        let dx = px as isize - (x1 - r) as isize;
        let dy = (y0 + r) as isize - py as isize;
        return sq(dx) + sq(dy) <= sq(r as isize);
    }
    if in_l && in_b {
        let dx = (x0 + r) as isize - px as isize;
        let dy = py as isize - (y1 - r) as isize;
        return sq(dx) + sq(dy) <= sq(r as isize);
    }
    if in_r && in_b {
        let dx = px as isize - (x1 - r) as isize;
        let dy = py as isize - (y1 - r) as isize;
        return sq(dx) + sq(dy) <= sq(r as isize);
    }
    true
}
