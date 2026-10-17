mod font_data;

pub const GLYPH_W: i32 = 8;
pub const GLYPH_H: i32 = 8;

pub fn glyph(ch: char) -> &'static [u8; 8] {
    let code = ch as u32;
    if code < 128 {
        &font_data::FONT8X8[code as usize]
    } else {
        &font_data::FONT8X8[b'?' as usize]
    }
}

/// Draw one glyph. Bit 0 of each row is the leftmost pixel.
#[allow(clippy::too_many_arguments)]
pub fn blit_glyph(
    fb: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    ch: char,
    color: [u8; 4],
) {
    let rows = glyph(ch);
    for row in 0..8 {
        let bits = rows[row as usize];
        for col in 0..8 {
            if (bits >> col) & 1 == 1 {
                put(fb, width, height, x + col, y + row, color);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_text(
    fb: &mut [u8],
    width: i32,
    height: i32,
    mut x: i32,
    y: i32,
    text: &str,
    color: [u8; 4],
) {
    for ch in text.chars() {
        blit_glyph(fb, width, height, x, y, ch, color);
        x += GLYPH_W;
    }
}

pub fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(1);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            if line.is_empty() {
                push_long(&mut lines, &mut line, word, max_chars);
            } else if line.chars().count() + 1 + word.chars().count() <= max_chars {
                line.push(' ');
                line.push_str(word);
            } else {
                lines.push(std::mem::take(&mut line));
                push_long(&mut lines, &mut line, word, max_chars);
            }
        }
        lines.push(std::mem::take(&mut line));
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn push_long(lines: &mut Vec<String>, line: &mut String, word: &str, max_chars: usize) {
    if word.chars().count() <= max_chars {
        line.push_str(word);
        return;
    }
    let mut rest = word;
    while rest.chars().count() > max_chars {
        let (head, tail) = split_chars(rest, max_chars);
        lines.push(head.to_string());
        rest = tail;
    }
    line.push_str(rest);
}

fn split_chars(text: &str, n: usize) -> (&str, &str) {
    let mut end = text.len();
    for (i, (idx, _)) in text.char_indices().enumerate() {
        if i == n {
            end = idx;
            break;
        }
    }
    text.split_at(end)
}

pub fn put(fb: &mut [u8], width: i32, height: i32, x: i32, y: i32, color: [u8; 4]) {
    if x < 0 || y < 0 || x >= width || y >= height {
        return;
    }
    let i = ((y as u32 * width as u32 + x as u32) * 4) as usize;
    fb[i..i + 4].copy_from_slice(&color);
}

#[allow(clippy::too_many_arguments)]
pub fn fill_rect(
    fb: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: [u8; 4],
) {
    for py in y..y + h {
        for px in x..x + w {
            put(fb, width, height, px, py, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_on_words() {
        let lines = wrap_text("one two three", 8);
        assert_eq!(lines, vec!["one two".to_string(), "three".to_string()]);
    }
}
