use crate::font::{self, GLYPH_H, GLYPH_W};
use crate::imageutil::{RgbaImage, SpriteFrame};
use crate::model::Band;

pub const BAR_H: i32 = 10;

const TEXT: [u8; 4] = [236, 232, 220, 255];
const YELLOW: [u8; 4] = [242, 214, 92, 255];
const BLACK: [u8; 4] = [8, 6, 12, 255];
const BAR: [u8; 4] = [12, 10, 22, 255];

pub struct Layer {
    pub id: String,
    pub priority: u8,
    pub sort_y: i32,
    pub dest_x: i32,
    pub dest_y: i32,
    pub sprite: SpriteFrame,
    /// Screen-sized mask. Nonzero pixels undo this sprite so a counter can sit in front of one actor.
    pub occlude: Option<Vec<u8>>,
}

pub fn band_for_y(bands: &[Band], y: i32) -> u8 {
    let mut priority = 0u8;
    for band in bands {
        if y >= band.y {
            priority = band.priority;
        }
    }
    priority
}

#[allow(clippy::too_many_arguments)]
pub fn composite(
    fb: &mut [u8],
    width: i32,
    height: i32,
    background: &RgbaImage,
    priority: &[u8],
    mut layers: Vec<Layer>,
    status_left: &str,
    status_right: &str,
    narrator: Option<&str>,
    input_line: &str,
) {
    blit_background(fb, width, height, background);
    layers.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then(a.sort_y.cmp(&b.sort_y))
            .then(a.id.cmp(&b.id))
    });
    for layer in layers {
        let before = if layer.occlude.is_some() {
            Some(fb.to_vec())
        } else {
            None
        };
        blit_sprite(fb, width, height, priority, &layer);
        if let (Some(before), Some(mask)) = (before, layer.occlude.as_deref()) {
            restore_occluded(fb, &before, mask);
        }
    }
    draw_status(fb, width, height, status_left, status_right);
    if let Some(text) = narrator {
        draw_narrator(fb, width, height, text);
    }
    draw_input(fb, width, height, input_line);
}

fn restore_occluded(fb: &mut [u8], before: &[u8], mask: &[u8]) {
    let pixels = fb.len() / 4;
    if before.len() != fb.len() || mask.len() != pixels {
        return;
    }
    for (i, bit) in mask.iter().enumerate() {
        if *bit != 0 {
            let di = i * 4;
            fb[di..di + 4].copy_from_slice(&before[di..di + 4]);
        }
    }
}

fn blit_background(fb: &mut [u8], width: i32, height: i32, background: &RgbaImage) {
    if background.w == width as u32
        && background.h == height as u32
        && background.rgba.len() == fb.len()
    {
        fb.copy_from_slice(&background.rgba);
        return;
    }
    for px in fb.as_chunks_mut::<4>().0 {
        px.copy_from_slice(&[0, 0, 0, 255]);
    }
}

/// Draw one sprite with the same alpha and priority test the compositor uses.
pub fn paint_sprite(fb: &mut [u8], width: i32, height: i32, priority: &[u8], layer: &Layer) {
    blit_sprite(fb, width, height, priority, layer);
}

fn blit_sprite(fb: &mut [u8], width: i32, height: i32, priority: &[u8], layer: &Layer) {
    let sprite = &layer.sprite;
    for row in 0..sprite.h as i32 {
        for col in 0..sprite.w as i32 {
            let dx = layer.dest_x + col;
            let dy = layer.dest_y + row;
            if dx < 0 || dy < 0 || dx >= width || dy >= height {
                continue;
            }
            let si = ((row as u32 * sprite.w + col as u32) * 4) as usize;
            if sprite.rgba[si + 3] < 128 {
                continue;
            }
            let pi = (dy as u32 * width as u32 + dx as u32) as usize;
            if priority.get(pi).copied().unwrap_or(0) > layer.priority {
                continue;
            }
            let di = pi * 4;
            fb[di] = sprite.rgba[si];
            fb[di + 1] = sprite.rgba[si + 1];
            fb[di + 2] = sprite.rgba[si + 2];
            fb[di + 3] = 255;
        }
    }
}

fn draw_status(fb: &mut [u8], width: i32, height: i32, left: &str, right: &str) {
    font::fill_rect(fb, width, height, 0, 0, width, BAR_H, BAR);
    font::draw_text(fb, width, height, 2, 1, left, TEXT);
    let chars = right.chars().count() as i32;
    let x = width - 2 - chars * GLYPH_W;
    font::draw_text(fb, width, height, x.max(2), 1, right, YELLOW);
}

fn draw_input(fb: &mut [u8], width: i32, height: i32, line: &str) {
    let y = height - BAR_H;
    font::fill_rect(fb, width, height, 0, y, width, BAR_H, BAR);
    let max = ((width / GLYPH_W) as usize).saturating_sub(2);
    let shown = tail_chars(line, max);
    let text = format!("> {shown}_");
    font::draw_text(fb, width, height, 2, y + 1, &text, YELLOW);
}

fn draw_narrator(fb: &mut [u8], width: i32, height: i32, text: &str) {
    let max_chars = ((width / GLYPH_W) as usize).saturating_sub(2).max(1);
    let lines = font::wrap_text(text, max_chars);
    let box_h = lines.len() as i32 * GLYPH_H + 8;
    let bottom = height - BAR_H;
    let mut top = bottom - box_h;
    if top < BAR_H {
        top = BAR_H;
    }
    font::fill_rect(fb, width, height, 0, top, width, bottom - top, BLACK);
    font::fill_rect(fb, width, height, 0, top, width, 1, TEXT);
    font::fill_rect(fb, width, height, 0, bottom - 1, width, 1, TEXT);
    font::fill_rect(fb, width, height, 0, top, 1, bottom - top, TEXT);
    font::fill_rect(fb, width, height, width - 1, top, 1, bottom - top, TEXT);
    for (i, line) in lines.iter().enumerate() {
        font::draw_text(
            fb,
            width,
            height,
            8,
            top + 4 + i as i32 * GLYPH_H,
            line,
            TEXT,
        );
    }
}

fn tail_chars(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    text.chars().skip(count - max).collect()
}

pub fn merge_priority(width: i32, height: i32, bands: &[Band], paint: Option<&[u8]>) -> Vec<u8> {
    let mut out = vec![0u8; (width * height) as usize];
    for y in 0..height {
        let band = band_for_y(bands, y);
        for x in 0..width {
            let i = (y * width + x) as usize;
            let painted = paint.and_then(|p| p.get(i).copied()).unwrap_or(0);
            out[i] = if painted != 0 { painted } else { band };
        }
    }
    out
}
