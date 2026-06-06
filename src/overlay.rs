use anyhow::Result;
use softbuffer::{Context, Surface};
use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::sync::Arc;
use winit::window::Window;

const N_BARS: usize = 75;
const WAVE_COLS: usize = 28;

// Palette (ARGB).
const PILL_BG: u32 = 0xFF1B1B1D;
const PILL_BORDER: u32 = 0xFF3A3A3C;
const REC_RED: u32 = 0xFFEA3A2E;
const DOT_HI: u32 = 0xFFF2F2F4;
const DOT_LO: u32 = 0xFF6E6E73;
const DOT_THINKING: u32 = 0xFFB0B0B8; // slightly dimmer for thinking dots

pub struct Overlay {
    pub window: Arc<Window>,
    _context: Context<Arc<Window>>,
    surface: Surface<Arc<Window>, Arc<Window>>,
    bars: VecDeque<f32>,
    recording: bool,
    transcribing: bool,
    frame: u32,
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
            transcribing: false,
            frame: 0,
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

    pub fn set_transcribing(&mut self, transcribing: bool) {
        self.transcribing = transcribing;
        if !transcribing {
            self.frame = 0;
        }
    }

    pub fn push_level(&mut self, level: f32) {
        self.bars.pop_front();
        self.bars.push_back(level.clamp(0.0, 1.0));
        self.frame = self.frame.wrapping_add(1);
    }

    /// Advance the animation frame counter (used during transcription ticks).
    pub fn tick(&mut self) {
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
            self.transcribing,
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
    transcribing: bool,
) {
    if w < 40 || h < 20 {
        return;
    }
    buf.fill(0x00000000);

    let pad = 6usize;
    let x0 = pad;
    let y0 = pad;
    let x1 = w - pad;
    let y1 = h - pad;
    let corner_r = (y1 - y0) / 2;

    // === Pill body ===
    for y in y0..y1 {
        for x in x0..x1 {
            if in_rounded_rect(x, y, x0, y0, x1, y1, corner_r) {
                buf[y * w + x] = PILL_BG;
            }
        }
    }

    // === 1px rim ===
    for y in y0..y1 {
        for x in x0..x1 {
            let outer = in_rounded_rect(x, y, x0, y0, x1, y1, corner_r);
            let inner = in_rounded_rect(
                x, y, x0 + 1, y0 + 1, x1 - 1, y1 - 1,
                corner_r.saturating_sub(1),
            );
            if outer && !inner {
                buf[y * w + x] = PILL_BORDER;
            }
        }
    }

    let cy = h / 2;

    // === Record button ===
    let btn_margin = pad + 8;
    let btn_size = (y1 - y0).saturating_sub(16).max(20);
    let bx0 = x0 + (btn_margin - pad);
    let by0 = cy.saturating_sub(btn_size / 2);
    let bx1 = bx0 + btn_size;
    let by1 = by0 + btn_size;
    let btn_r = (btn_size / 3).max(6);

    let pulse = ((frame as f32 * 0.14).sin() * 0.18 + 0.82).clamp(0.0, 1.0);
    // While transcribing, dim the button to gray to signal "not recording".
    let btn_color = if transcribing {
        scale_rgb(0xFF555558, 0.9 + 0.1 * pulse)
    } else {
        scale_rgb(REC_RED, 0.78 + 0.22 * pulse)
    };

    for y in by0..by1 {
        for x in bx0..bx1 {
            if in_rounded_rect(x, y, bx0, by0, bx1, by1, btn_r) {
                buf[y * w + x] = btn_color;
            }
        }
    }
    draw_logo_triangle(buf, w, h, bx0, by0, btn_size);

    // === Right area: waveform or thinking indicator ===
    let wave_x0 = bx1 + 18;
    let wave_x1 = x1.saturating_sub(18);
    if wave_x1 <= wave_x0 {
        return;
    }

    if transcribing {
        draw_thinking_dots(buf, w, h, wave_x0, wave_x1, cy, frame);
    } else {
        draw_waveform(buf, w, h, bars, wave_x0, wave_x1, cy, y0, y1, pad);
    }
}

/// Three cascading dots that pulse left-to-right while Whisper is transcribing.
fn draw_thinking_dots(
    buf: &mut [u32],
    w: usize,
    h: usize,
    x0: usize,
    x1: usize,
    cy: usize,
    frame: u32,
) {
    let spacing = 14usize;
    let total_w = spacing * 2;
    let center = (x0 + x1) / 2;
    let start_x = center.saturating_sub(total_w / 2);

    for i in 0..3usize {
        let px = start_x + i * spacing;
        // Each dot is offset by 8 frames out of a 24-frame cycle.
        let phase = (frame as usize + i * 8) % 24;
        let t = phase as f32 / 24.0;
        // Smooth sine pulse: dim→bright→dim over one cycle.
        let brightness = 0.30 + 0.70 * (std::f32::consts::PI * t).sin();
        let col = scale_rgb(DOT_THINKING, brightness);
        draw_dot(buf, w, h, px, cy, col);
    }
}

/// Dotted waveform that reacts to the audio level during recording.
fn draw_waveform(
    buf: &mut [u32],
    w: usize,
    h: usize,
    bars: &VecDeque<f32>,
    wave_x0: usize,
    wave_x1: usize,
    cy: usize,
    y0: usize,
    y1: usize,
    pad: usize,
) {
    if bars.is_empty() {
        return;
    }
    let n = bars.len();
    let cols = WAVE_COLS.min((wave_x1 - wave_x0) / 5).max(1);
    let step = (wave_x1 - wave_x0) / cols;
    let dot_gap = 6usize;
    let max_dots = ((h / 2).saturating_sub(pad + 6) / dot_gap).max(1);

    for c in 0..cols {
        let bi = if cols > 1 { c * (n - 1) / (cols - 1) } else { 0 };
        let level = bars[bi];
        let shaped = level.powf(0.6);
        let extra = (shaped * max_dots as f32).round() as usize;
        let dx = wave_x0 + c * step + step / 2;

        let active = extra > 0;
        draw_dot(buf, w, h, dx, cy, if active { DOT_HI } else { DOT_LO });
        for k in 1..=extra {
            let off = k * dot_gap;
            let fade = 1.0 - (k as f32 / (max_dots as f32 + 1.0)) * 0.55;
            let col = scale_rgb(DOT_HI, fade);
            if cy + off < y1 {
                draw_dot(buf, w, h, dx, cy + off, col);
            }
            if cy >= off + y0 {
                draw_dot(buf, w, h, dx, cy - off, col);
            }
        }
    }
}

fn draw_dot(buf: &mut [u32], w: usize, h: usize, cx: usize, cy: usize, color: u32) {
    let r = 1.6f32;
    let ri = 2usize;
    let rgb = color & 0x00FFFFFF;
    for y in cy.saturating_sub(ri)..=(cy + ri).min(h.saturating_sub(1)) {
        for x in cx.saturating_sub(ri)..=(cx + ri).min(w.saturating_sub(1)) {
            let d = (((x as f32 - cx as f32).powi(2)) + ((y as f32 - cy as f32).powi(2))).sqrt();
            if d <= r {
                buf[y * w + x] = blend_over(color, buf[y * w + x]);
            } else if d < r + 1.0 {
                let a = (((1.0 - (d - r)) * ((color >> 24) as f32)).clamp(0.0, 255.0)) as u32;
                buf[y * w + x] = blend_over((a << 24) | rgb, buf[y * w + x]);
            }
        }
    }
}

fn draw_logo_triangle(buf: &mut [u32], w: usize, h: usize, bx0: usize, by0: usize, size: usize) {
    let cx = bx0 as f32 + size as f32 / 2.0;
    let cy = by0 as f32 + size as f32 / 2.0;
    let s = size as f32 * 0.30;

    let ang = [-90f32, 30f32, 150f32];
    let pts: Vec<(f32, f32)> = ang
        .iter()
        .map(|a| {
            let r = a.to_radians();
            (cx + s * r.cos(), cy + s * 0.18 + s * r.sin())
        })
        .collect();

    let minx = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min).floor() as usize;
    let maxx = pts.iter().map(|p| p.0).fold(0.0, f32::max).ceil() as usize;
    let miny = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor() as usize;
    let maxy = pts.iter().map(|p| p.1).fold(0.0, f32::max).ceil() as usize;

    for y in miny..=maxy.min(h.saturating_sub(1)) {
        for x in minx..=maxx.min(w.saturating_sub(1)) {
            let mut cover = 0u32;
            for sy in 0..2 {
                for sx in 0..2 {
                    let px = x as f32 + 0.25 + sx as f32 * 0.5;
                    let py = y as f32 + 0.25 + sy as f32 * 0.5;
                    if point_in_tri(px, py, pts[0], pts[1], pts[2]) {
                        cover += 1;
                    }
                }
            }
            if cover > 0 {
                let a = (cover * 255 / 4) & 0xFF;
                buf[y * w + x] = blend_over((a << 24) | 0x00FFFFFF, buf[y * w + x]);
            }
        }
    }
}

fn point_in_tri(px: f32, py: f32, a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    let d1 = sign(px, py, a, b);
    let d2 = sign(px, py, b, c);
    let d3 = sign(px, py, c, a);
    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_neg && has_pos)
}

fn sign(px: f32, py: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    (px - b.0) * (a.1 - b.1) - (a.0 - b.0) * (py - b.1)
}

fn scale_rgb(c: u32, f: f32) -> u32 {
    let a = c & 0xFF000000;
    let r = (((c >> 16) & 0xFF) as f32 * f).clamp(0.0, 255.0) as u32;
    let g = (((c >> 8) & 0xFF) as f32 * f).clamp(0.0, 255.0) as u32;
    let b = ((c & 0xFF) as f32 * f).clamp(0.0, 255.0) as u32;
    a | (r << 16) | (g << 8) | b
}

fn blend_over(fg: u32, bg: u32) -> u32 {
    let a = (fg >> 24) & 0xFF;
    if a == 0 { return bg; }
    if a == 255 { return fg; }
    let r = ((((fg >> 16) & 0xFF) * a + ((bg >> 16) & 0xFF) * (255 - a)) / 255) as u32;
    let g = ((((fg >> 8) & 0xFF) * a + ((bg >> 8) & 0xFF) * (255 - a)) / 255) as u32;
    let b = (((fg & 0xFF) * a + (bg & 0xFF) * (255 - a)) / 255) as u32;
    0xFF000000 | (r << 16) | (g << 8) | b
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
