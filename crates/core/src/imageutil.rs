use crate::error::{Error, Result};
use std::io::Cursor;

#[derive(Debug, Clone)]
pub struct RgbaImage {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct GrayImage {
    pub w: u32,
    pub h: u32,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct SpriteFrame {
    pub w: u32,
    pub h: u32,
    pub ax: i32,
    pub ay: i32,
    pub rgba: Vec<u8>,
}

impl RgbaImage {
    pub fn solid(w: u32, h: u32, color: [u8; 4]) -> Self {
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            rgba.extend_from_slice(&color);
        }
        Self { w, h, rgba }
    }

    pub fn pixel(&self, x: i32, y: i32) -> Option<[u8; 4]> {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return None;
        }
        let i = ((y as u32 * self.w + x as u32) * 4) as usize;
        Some([
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ])
    }
}

impl GrayImage {
    pub fn solid(w: u32, h: u32, value: u8) -> Self {
        Self {
            w,
            h,
            pixels: vec![value; (w * h) as usize],
        }
    }
}

pub fn encode_rgba(img: &RgbaImage) -> Result<Vec<u8>> {
    encode(img.w, img.h, png::ColorType::Rgba, &img.rgba)
}

pub fn encode_gray(img: &GrayImage) -> Result<Vec<u8>> {
    encode(img.w, img.h, png::ColorType::Grayscale, &img.pixels)
}

fn encode(w: u32, h: u32, color: png::ColorType, data: &[u8]) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut encoder = png::Encoder::new(&mut buf, w, h);
    encoder.set_color(color);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|err| Error::msg(err.to_string()))?;
    writer
        .write_image_data(data)
        .map_err(|err| Error::msg(err.to_string()))?;
    drop(writer);
    Ok(buf)
}

pub fn decode_png(bytes: &[u8]) -> Result<Decoded> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|err| Error::msg(err.to_string()))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|err| Error::msg(err.to_string()))?;
    let data = &buf[..info.buffer_size()];
    let w = info.width;
    let h = info.height;
    match info.color_type {
        png::ColorType::Rgba => Ok(Decoded::Rgba(RgbaImage {
            w,
            h,
            rgba: data.to_vec(),
        })),
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for px in data.as_chunks::<3>().0 {
                rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            Ok(Decoded::Rgba(RgbaImage { w, h, rgba }))
        }
        png::ColorType::Grayscale => Ok(Decoded::Gray(GrayImage {
            w,
            h,
            pixels: data.to_vec(),
        })),
        png::ColorType::GrayscaleAlpha => {
            let mut pixels = Vec::with_capacity((w * h) as usize);
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for px in data.as_chunks::<2>().0 {
                pixels.push(px[0]);
                rgba.extend_from_slice(&[px[0], px[0], px[0], px[1]]);
            }
            Ok(Decoded::GrayAlpha {
                gray: GrayImage { w, h, pixels },
                rgba: RgbaImage { w, h, rgba },
            })
        }
        other => Err(Error::msg(format!("unsupported png color type {other:?}"))),
    }
}

pub enum Decoded {
    Rgba(RgbaImage),
    Gray(GrayImage),
    GrayAlpha { gray: GrayImage, rgba: RgbaImage },
}

impl Decoded {
    pub fn into_rgba(self) -> RgbaImage {
        match self {
            Decoded::Rgba(img) => img,
            Decoded::Gray(img) => {
                let mut rgba = Vec::with_capacity(img.pixels.len() * 4);
                for p in img.pixels {
                    rgba.extend_from_slice(&[p, p, p, 255]);
                }
                RgbaImage {
                    w: img.w,
                    h: img.h,
                    rgba,
                }
            }
            Decoded::GrayAlpha { rgba, .. } => rgba,
        }
    }

    pub fn into_gray(self) -> GrayImage {
        match self {
            Decoded::Gray(img) => img,
            Decoded::GrayAlpha { gray, .. } => gray,
            Decoded::Rgba(img) => {
                let mut pixels = Vec::with_capacity((img.w * img.h) as usize);
                for px in img.rgba.as_chunks::<4>().0 {
                    pixels.push(px[0]);
                }
                GrayImage {
                    w: img.w,
                    h: img.h,
                    pixels,
                }
            }
        }
    }
}

pub fn crop(
    atlas: &RgbaImage,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    ax: i32,
    ay: i32,
) -> Result<SpriteFrame> {
    if w == 0 || h == 0 || x.saturating_add(w) > atlas.w || y.saturating_add(h) > atlas.h {
        return Err(Error::msg("sprite frame sits outside its atlas"));
    }
    if ax < 0 || ay < 0 || ax >= w as i32 || ay >= h as i32 {
        return Err(Error::msg(format!(
            "anchor ({ax},{ay}) is outside a {w}x{h} frame"
        )));
    }
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for row in 0..h {
        let src = (((y + row) * atlas.w + x) * 4) as usize;
        let dst = (row * w * 4) as usize;
        let n = (w * 4) as usize;
        rgba[dst..dst + n].copy_from_slice(&atlas.rgba[src..src + n]);
    }
    Ok(SpriteFrame { w, h, ax, ay, rgba })
}
