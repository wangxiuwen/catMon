//! 内置角色帧。单色风格：墨色由调用方按托盘深浅传入（深底浅墨 / 浅底深墨）。
//! 眼睛、舷窗等负空间用挖洞实现。坐标基于 32x32 逻辑网格，y 向下。
//! 应用图标仍用 AMBER 彩色，与托盘无关。

use crate::draw::{rgba, Canvas, Rgba};

/// 应用图标用的彩色
pub const AMBER: Rgba = rgba(235, 165, 66, 255);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    /// 多帧循环动画
    Animated,
    /// 静态/按外部数据重绘（如数字角色）
    Static,
}

#[derive(Clone)]
pub struct Character {
    pub id: String,
    pub label: String,
    pub kind: Kind,
    /// 每帧 BASE*BASE*4 的 RGBA
    pub frames: Vec<Vec<u8>>,
}

/// 按墨色构建内置角色
pub fn builtin(ink: Rgba) -> Vec<Character> {
    vec![
        Character {
            id: "cat".into(),
            label: "猫".into(),
            kind: Kind::Animated,
            frames: (0..4).map(|f| run_cat(f, ink)).collect(),
        },
        Character {
            id: "ghost".into(),
            label: "幽灵".into(),
            kind: Kind::Animated,
            frames: (0..2).map(|f| ghost(f, ink)).collect(),
        },
        Character {
            id: "rocket".into(),
            label: "火箭".into(),
            kind: Kind::Animated,
            frames: (0..4).map(|f| rocket(f, ink)).collect(),
        },
        Character {
            id: "digits".into(),
            label: "数字（跟随指标）".into(),
            kind: Kind::Static,
            frames: vec![digits(0.0, ink)],
        },
    ]
}

// ---- 猫：4 帧小跑 ----
// 每条腿 [膝x, 膝y, 脚x, 脚y]（相对髋部）；顺序：前A 前B 后A 后B
const CAT_LEGS: [[f32; 16]; 4] = [
    [2.0, 2.5, 4.5, 5.5, -0.5, 2.5, -1.5, 5.3, -2.0, 2.5, -4.5, 5.0, 0.5, 2.5, 1.5, 5.5],
    [1.0, 2.5, 2.0, 5.5, 1.0, 2.5, 2.5, 5.3, -1.0, 2.5, -2.0, 5.5, -1.0, 2.5, -3.0, 5.0],
    [-0.5, 2.5, -1.0, 5.5, 2.0, 2.5, 4.5, 5.0, 0.5, 2.5, 1.0, 5.5, -2.0, 2.5, -4.5, 5.0],
    [1.0, 2.5, 3.0, 5.5, 1.0, 2.5, 2.0, 5.3, -1.0, 2.5, -2.5, 5.5, -1.0, 2.5, -2.0, 5.5],
];
const CAT_HIPS: [[f32; 2]; 4] = [[19.0, 20.5], [21.5, 20.3], [11.5, 20.8], [13.5, 20.8]];

fn run_cat(frame: usize, ink: Rgba) -> Vec<u8> {
    let mut c = Canvas::new();
    let f = frame & 3;
    let bob = [0.0, -0.8, 0.0, 0.8][f];
    let wag = [0.0, 1.6, 3.0, 1.6][f];
    // 尾巴
    c.line(10.5, 17.5 + bob, 6.0, 13.0 + bob, 2.2, ink);
    c.line(6.0, 13.0 + bob, 4.2, 8.0 + wag + bob, 2.0, ink);
    // 腿（先画，上端被身体压住）
    for (li, hip) in CAT_HIPS.iter().enumerate() {
        let o = li * 4;
        let (kx, ky, fx, fy) = (CAT_LEGS[f][o], CAT_LEGS[f][o + 1], CAT_LEGS[f][o + 2], CAT_LEGS[f][o + 3]);
        c.line(hip[0], hip[1] + bob, hip[0] + kx, hip[1] + ky + bob, 2.4, ink);
        c.line(hip[0] + kx, hip[1] + ky + bob, hip[0] + fx, hip[1] + fy + bob, 2.2, ink);
    }
    // 身体
    c.polyline(&[(10.0, 17.5 + bob), (15.5, 16.2 + bob), (20.5, 16.8 + bob)], 6.5, ink);
    // 头 + 耳朵
    c.disc(25.0, 11.2 + bob, 4.3, ink);
    c.poly(&[(21.6, 9.4 + bob), (22.7, 4.8 + bob), (25.1, 8.0 + bob)], ink);
    c.poly(&[(25.3, 8.0 + bob), (27.5, 4.8 + bob), (28.6, 9.6 + bob)], ink);
    // 眼睛：负空间
    c.clear_disc(26.6, 10.8 + bob, 0.9);
    c.finish()
}

fn ghost(frame: usize, ink: Rgba) -> Vec<u8> {
    let mut c = Canvas::new();
    let dy = if frame & 1 == 0 { 0.0 } else { 1.4 };
    let spread = if frame & 1 == 0 { 0.0 } else { 1.2 };
    c.disc(16.0, 11.0 + dy, 7.0, ink);
    c.rect(9.0, 11.0 + dy, 14.0, 9.5, ink);
    for i in 0..3u32 {
        let x0 = 9.0 + i as f32 * 5.0;
        let dyp = if (i + frame as u32) % 2 == 0 { 3.2 + spread } else { 1.8 };
        c.poly(&[(x0, 20.0 + dy), (x0 + 2.5, 20.0 + dyp + dy), (x0 + 5.0, 20.0 + dy)], ink);
    }
    c.clear_disc(13.2, 11.0 + dy, 1.5);
    c.clear_disc(18.8, 11.0 + dy, 1.5);
    c.finish()
}

fn rocket(frame: usize, ink: Rgba) -> Vec<u8> {
    let mut c = Canvas::new();
    let f = frame & 3;
    let flame = [5.0, 8.5, 6.5, 7.5][f];
    let oy = [0.0, -0.6, 0.2, -0.3][f];
    // 尾焰
    c.poly(&[(13.0, 19.5 + oy), (16.0, 19.5 + flame + oy), (19.0, 19.5 + oy)], ink);
    // 机身
    c.poly(&[(16.0, 2.5 + oy), (20.8, 10.5 + oy), (11.2, 10.5 + oy)], ink);
    c.rect(11.2, 10.5 + oy, 9.6, 9.0, ink);
    // 尾翼
    c.poly(&[(11.2, 14.5 + oy), (7.6, 21.0 + oy), (11.2, 19.5 + oy)], ink);
    c.poly(&[(20.8, 14.5 + oy), (24.4, 21.0 + oy), (20.8, 19.5 + oy)], ink);
    // 舷窗：负空间
    c.clear_disc(16.0, 12.0 + oy, 2.2);
    c.finish()
}

// ---- 数字角色：托盘直接显示 CPU% + 底部进度条 ----

/// 3x5 像素字体，每行 3 bit，高位在左
fn glyph(ch: u8) -> [u8; 5] {
    match ch {
        b'0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        b'1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        b'2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        b'3' => [0b111, 0b001, 0b011, 0b001, 0b111],
        b'4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        b'5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        b'6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        b'7' => [0b111, 0b001, 0b001, 0b010, 0b010],
        b'8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        b'9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        b'%' => [0b101, 0b001, 0b010, 0b100, 0b101],
        _ => [0; 5],
    }
}

pub fn digits(pct: f32, ink: Rgba) -> Vec<u8> {
    let mut c = Canvas::new();
    let p = pct.clamp(0.0, 100.0);
    let text = if p >= 99.5 { "100".to_string() } else { format!("{:.0}", p) };
    let s = 3.0f32;
    let gap = 1.0f32;
    let w = text.len() as f32 * (3.0 * s + gap) - gap;
    let mut x = ((crate::draw::BASE as f32 - w) / 2.0).round();
    for b in text.bytes() {
        for (r, row) in glyph(b).iter().enumerate() {
            for col in 0..3u32 {
                if row & (0b100 >> col) != 0 {
                    c.rect(x + col as f32 * s, 5.0 + r as f32 * s, s, s, ink);
                }
            }
        }
        x += 3.0 * s + gap;
    }
    // 底部进度条
    c.rect(3.0, 26.0, (crate::draw::BASE as f32 - 6.0) * p / 100.0, 2.5, ink);
    c.finish()
}
