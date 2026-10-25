//! 极简软件光栅化器：把角色帧绘制进 RGBA 缓冲。
//! 逻辑坐标固定 32x32，内部 2x 超采样再缩回，边缘更平滑。

pub const BASE: u32 = 32;
const SS: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Rgba {
    Rgba { r, g, b, a }
}

pub struct Canvas {
    w: u32,
    h: u32,
    px: Vec<u8>,
}

impl Canvas {
    pub fn new() -> Self {
        let s = BASE * SS;
        Self {
            w: s,
            h: s,
            px: vec![0; (s * s * 4) as usize],
        }
    }

    fn blend(&mut self, x: u32, y: u32, c: Rgba) {
        if x >= self.w || y >= self.h || c.a == 0 {
            return;
        }
        let i = ((y * self.w + x) * 4) as usize;
        let a = c.a as u32;
        let ia = 255 - a;
        let mix = |v: u8, s: u32| ((s * a + v as u32 * ia) / 255) as u8;
        self.px[i] = mix(self.px[i], c.r as u32);
        self.px[i + 1] = mix(self.px[i + 1], c.g as u32);
        self.px[i + 2] = mix(self.px[i + 2], c.b as u32);
        self.px[i + 3] = ((a * 255 + self.px[i + 3] as u32 * ia) / 255) as u8;
    }

    fn disc_ss(&mut self, cx: f32, cy: f32, r: f32, c: Rgba) {
        let x0 = ((cx - r).max(0.0) as i32).min(self.w as i32);
        let y0 = ((cy - r).max(0.0) as i32).min(self.h as i32);
        let x1 = (((cx + r) + 1.0) as i32).min(self.w as i32);
        let y1 = (((cy + r) + 1.0) as i32).min(self.h as i32);
        for y in y0..y1 {
            for x in x0..x1 {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                if dx * dx + dy * dy <= r * r {
                    self.blend(x as u32, y as u32, c);
                }
            }
        }
    }

    /// 实心圆（逻辑坐标）
    pub fn disc(&mut self, cx: f32, cy: f32, r: f32, c: Rgba) {
        self.disc_ss(cx * SS as f32, cy * SS as f32, r * SS as f32, c);
    }

    /// 圆头粗线段
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, c: Rgba) {
        let (ax, ay) = (x0 * SS as f32, y0 * SS as f32);
        let (bx, by) = (x1 * SS as f32, y1 * SS as f32);
        let (dx, dy) = (bx - ax, by - ay);
        let steps = ((dx * dx + dy * dy).sqrt().ceil() as u32).max(1);
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            self.disc_ss(ax + dx * t, ay + dy * t, width * SS as f32 / 2.0, c);
        }
    }

    /// 折线
    pub fn polyline(&mut self, pts: &[(f32, f32)], width: f32, c: Rgba) {
        for w in pts.windows(2) {
            self.line(w[0].0, w[0].1, w[1].0, w[1].1, width, c);
        }
    }

    /// 多边形填充（奇偶规则，逻辑坐标）
    pub fn poly(&mut self, pts: &[(f32, f32)], c: Rgba) {
        if pts.len() < 3 {
            return;
        }
        let s = SS as f32;
        let sp: Vec<(f32, f32)> = pts.iter().map(|(x, y)| (x * s, y * s)).collect();
        let minx = sp.iter().fold(f32::MAX, |m, p| m.min(p.0)).floor().max(0.0) as i32;
        let miny = sp.iter().fold(f32::MAX, |m, p| m.min(p.1)).floor().max(0.0) as i32;
        let maxx = (sp.iter().fold(f32::MIN, |m, p| m.max(p.0)).ceil() as i32).min(self.w as i32);
        let maxy = (sp.iter().fold(f32::MIN, |m, p| m.max(p.1)).ceil() as i32).min(self.h as i32);
        for y in miny..maxy {
            for x in minx..maxx {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let mut inside = false;
                let mut j = sp.len() - 1;
                for i in 0..sp.len() {
                    let (xi, yi) = sp[i];
                    let (xj, yj) = sp[j];
                    if (yi > py) != (yj > py)
                        && px < (xj - xi) * (py - yi) / (yj - yi) + xi
                    {
                        inside = !inside;
                    }
                    j = i;
                }
                if inside {
                    self.blend(x as u32, y as u32, c);
                }
            }
        }
    }

    /// 矩形填充
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgba) {
        self.poly(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)], c);
    }

    /// 在圆内清空成全透明（挖洞：眼睛、舷窗等负空间）
    pub fn clear_disc(&mut self, cx: f32, cy: f32, r: f32) {
        let (cx, cy, r) = (cx * SS as f32, cy * SS as f32, r * SS as f32);
        let x0 = ((cx - r).max(0.0) as i32).min(self.w as i32);
        let y0 = ((cy - r).max(0.0) as i32).min(self.h as i32);
        let x1 = (((cx + r) + 1.0) as i32).min(self.w as i32);
        let y1 = (((cy + r) + 1.0) as i32).min(self.h as i32);
        for y in y0..y1 {
            for x in x0..x1 {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                if dx * dx + dy * dy <= r * r {
                    let i = ((y as u32 * self.w + x as u32) * 4) as usize;
                    self.px[i..i + 4].fill(0);
                }
            }
        }
    }

    /// 2x2 均值缩回 32x32，返回 RGBA
    pub fn finish(&self) -> Vec<u8> {
        let s = BASE;
        let mut out = vec![0u8; (s * s * 4) as usize];
        for y in 0..s {
            for x in 0..s {
                for ch in 0..4 {
                    let mut acc = 0u32;
                    for dy in 0..SS {
                        for dx in 0..SS {
                            let i =
                                (((y * SS + dy) * self.w + (x * SS + dx)) * 4 + ch) as usize;
                            acc += self.px[i] as u32;
                        }
                    }
                    out[((y * s + x) * 4 + ch) as usize] = (acc / (SS * SS)) as u8;
                }
            }
        }
        out
    }
}
