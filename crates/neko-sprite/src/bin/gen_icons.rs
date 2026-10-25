//! 生成应用图标与预览拼图：`cargo run -p neko-sprite --bin gen-icons`
use std::fs;
use std::path::PathBuf;

use image::{ExtendedColorType, ImageEncoder};
use neko_sprite::draw::{rgba, Rgba};
use neko_sprite::sprites::{self, Character};

fn encode_png(buf: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(buf, w, h, ExtendedColorType::Rgba8)
        .expect("png encode");
    out
}

fn save_png(path: PathBuf, buf: &[u8], w: u32, h: u32) {
    fs::write(&path, encode_png(buf, w, h)).expect("write png");
    println!("  {}", path.display());
}

fn upscale(src: &[u8], base: u32, k: u32) -> Vec<u8> {
    let n = base * k;
    let mut out = vec![0u8; (n * n * 4) as usize];
    for y in 0..n {
        for x in 0..n {
            let si = (((y / k) * base + (x / k)) * 4) as usize;
            let di = ((y * n + x) * 4) as usize;
            out[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    out
}

/// 最近邻重采样，升/降采样通吃（16px 图标从 32px 取半采样）
fn nearest(src: &[u8], base: u32, dst: u32) -> Vec<u8> {
    let mut out = vec![0u8; (dst * dst * 4) as usize];
    for y in 0..dst {
        for x in 0..dst {
            let sx = x * base / dst;
            let sy = y * base / dst;
            let si = ((sy * base + sx) * 4) as usize;
            let di = ((y * dst + x) * 4) as usize;
            out[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    out
}

/// 手写多尺寸 ICO（PNG 压缩条目，Vista+ 支持；256 的宽高位写 0）
fn write_ico_multi(path: PathBuf, frames: &[u8], base: u32, sizes: &[u32]) {
    let mut entries: Vec<(u32, Vec<u8>)> = Vec::new();
    for &s in sizes {
        let img = nearest(frames, base, s);
        entries.push((s, encode_png(&img, s, s)));
    }
    let n = entries.len() as u16;
    let mut v = Vec::with_capacity(6 + 16 * entries.len() + entries.iter().map(|e| e.1.len()).sum::<usize>());
    v.extend_from_slice(&[0, 0, 1, 0]);
    v.extend_from_slice(&n.to_le_bytes());
    v.resize(6 + 16 * entries.len(), 0); // 目录区预填零，供下方索引赋值
    let mut offset = (6 + 16 * entries.len()) as u32;
    for (i, (s, png)) in entries.iter().enumerate() {
        let sb = if *s == 256 { 0u8 } else { *s as u8 };
        let e = 6 + i * 16;
        v[e] = sb;
        v[e + 1] = sb;
        v[e + 2] = 0;
        v[e + 3] = 0;
        v[e + 4..e + 6].copy_from_slice(&1u16.to_le_bytes());
        v[e + 6..e + 8].copy_from_slice(&32u16.to_le_bytes());
        v[e + 8..e + 12].copy_from_slice(&(png.len() as u32).to_le_bytes());
        v[e + 12..e + 16].copy_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in &entries {
        v.extend_from_slice(png);
    }
    fs::write(&path, v).expect("write ico");
    println!("  {}", path.display());
}

fn blit(img: &mut [u8], w: u32, src: &[u8], base: u32, x0: u32, y0: u32, k: u32) {
    for y in 0..base {
        for x in 0..base {
            let si = ((y * base + x) * 4) as usize;
            if src[si + 3] < 8 {
                continue;
            }
            for dy in 0..k {
                for dx in 0..k {
                    let di = (((y0 + y * k + dy) * w + (x0 + x * k + dx)) * 4) as usize;
                    img[di] = src[si];
                    img[di + 1] = src[si + 1];
                    img[di + 2] = src[si + 2];
                    img[di + 3] = 255;
                }
            }
        }
    }
}

/// 每个角色的预览帧行；数字角色补两档示例值
fn rows_for(chars: &[Character], ink: Rgba) -> Vec<Vec<Vec<u8>>> {
    chars
        .iter()
        .map(|c| {
            let mut f = c.frames.clone();
            if c.id == "digits" {
                f.push(sprites::digits(37.0, ink));
                f.push(sprites::digits(86.0, ink));
            }
            f
        })
        .collect()
}

/// 预览拼图：每个角色两行——浅底配深墨（浅色任务栏）/ 深底配浅墨（深色任务栏），8 倍放大
fn preview(ink_dark: Rgba, ink_light: Rgba, path: PathBuf) {
    let scale = 8u32;
    let cell = 32 * scale;
    let pad = 16u32;
    let rows_a = rows_for(&sprites::builtin(ink_dark), ink_dark);
    let rows_b = rows_for(&sprites::builtin(ink_light), ink_light);
    let n_rows = rows_a.len();
    let cols = rows_a
        .iter()
        .chain(rows_b.iter())
        .map(|r| r.len())
        .max()
        .unwrap_or(1) as u32;
    let w = pad * 2 + cols * (cell + pad);
    let rowh = cell + pad;
    let h = pad + (n_rows as u32) * 2 * rowh;
    let mut img = vec![0u8; (w * h * 4) as usize];
    for ri in 0..n_rows {
        for (rows, half) in [(&rows_a, 0u32), (&rows_b, 1u32)] {
            let y0 = pad + (ri as u32 * 2 + half) * rowh;
            let bg: [u8; 3] = if half == 0 { [234, 234, 238] } else { [30, 34, 46] };
            for y in y0..y0 + rowh {
                for x in 0..w {
                    let di = ((y * w + x) * 4) as usize;
                    img[di] = bg[0];
                    img[di + 1] = bg[1];
                    img[di + 2] = bg[2];
                    img[di + 3] = 255;
                }
            }
            for (fi, f) in rows[ri].iter().enumerate().take(cols as usize) {
                blit(&mut img, w, f, 32, pad + fi as u32 * (cell + pad), y0 + pad / 2, scale);
            }
        }
    }
    save_png(path, &img, w, h);
}

fn main() {
    let ws = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../");
    let icons = ws.join("src-tauri/icons");
    fs::create_dir_all(&icons).unwrap();
    println!("生成图标 → {}", icons.display());

    // 应用图标保持彩色猫
    let colored = sprites::builtin(sprites::AMBER);
    let cat = colored.iter().find(|c| c.id == "cat").unwrap();

    save_png(icons.join("32x32.png"), &cat.frames[1], 32, 32);
    save_png(icons.join("128x128.png"), &upscale(&cat.frames[1], 32, 4), 128, 128);
    save_png(icons.join("128x128@2x.png"), &upscale(&cat.frames[1], 32, 8), 256, 256);
    save_png(icons.join("icon.png"), &upscale(&cat.frames[1], 32, 16), 512, 512);
    write_ico_multi(
        icons.join("icon.ico"),
        &cat.frames[1],
        32,
        &[16, 32, 48, 256],
    );

    println!("生成预览拼图");
    preview(rgba(26, 26, 26, 255), rgba(235, 235, 235, 255), ws.join("preview.png"));
}
