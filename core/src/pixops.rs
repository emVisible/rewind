//! 自研像素算法:S5 CRT 显示模拟、S6 胶片机械损伤。就地处理 rgb24 帧缓冲。
//! 逐行可分片的循环走 rayon;划痕/尘埃是稀疏叠加,串行更便宜。

use rayon::prelude::*;

pub struct Xorshift(u64);

impl Xorshift {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 32) as u32
    }
    /// [0,1)
    pub fn f(&mut self) -> f64 {
        self.next_u32() as f64 / u32::MAX as f64
    }
    pub fn range(&mut self, lo: usize, hi: usize) -> usize {
        if hi <= lo {
            lo
        } else {
            lo + (self.next_u32() as usize % (hi - lo))
        }
    }
}

fn px(buf: &[u8], w: usize, x: usize, y: usize) -> (u8, u8, u8) {
    let i = (y * w + x) * 3;
    (buf[i], buf[i + 1], buf[i + 2])
}

fn bilinear(src: &[u8], w: usize, h: usize, fx: f64, fy: f64) -> (u8, u8, u8) {
    let x0 = fx.max(0.0).min(w as f64 - 1.001);
    let y0 = fy.max(0.0).min(h as f64 - 1.001);
    let (ix, iy) = (x0 as usize, y0 as usize);
    let tx = x0 - ix as f64;
    let ty = y0 - iy as f64;
    let x1 = (ix + 1).min(w - 1);
    let y1 = (iy + 1).min(h - 1);
    let g = |xx: usize, yy: usize, c: usize| -> f64 {
        let (r, gr, b) = px(src, w, xx, yy);
        [r, gr, b][c] as f64
    };
    let mut out = [0u8; 3];
    for c in 0..3 {
        let v = g(ix, iy, c) * (1.0 - tx) * (1.0 - ty)
            + g(x1, iy, c) * tx * (1.0 - ty)
            + g(ix, y1, c) * (1.0 - tx) * ty
            + g(x1, y1, c) * tx * ty;
        out[c] = v.round().clamp(0.0, 255.0) as u8;
    }
    (out[0], out[1], out[2])
}

/// S5:CRT 显示。barrel=桶形失真,scanline=扫描线深度,aberration=色散像素偏移
pub fn crt(buf: &mut [u8], w: usize, h: usize, scanline: f64, barrel: f64, aberration: f64) {
    let row = w * 3;
    if barrel > 0.01 {
        let src = buf.to_vec();
        let k = barrel * 0.45;
        buf.par_chunks_mut(row).enumerate().for_each(|(y, drow)| {
            for x in 0..w {
                let cx = (x as f64 + 0.5) / w as f64 - 0.5;
                let cy = (y as f64 + 0.5) / h as f64 - 0.5;
                let r2 = cx * cx + cy * cy;
                let s = 1.0 - k * r2 * 2.2;
                let fx = (cx / s + 0.5) * w as f64 - 0.5;
                let fy = (cy / s + 0.5) * h as f64 - 0.5;
                let (r, g, b) = bilinear(&src, w, h, fx, fy);
                let i = x * 3;
                drow[i] = r;
                drow[i + 1] = g;
                drow[i + 2] = b;
            }
        });
    }
    if aberration > 0.01 {
        let d = (aberration * 3.0).round() as usize;
        if d > 0 {
            let src = buf.to_vec();
            buf.par_chunks_mut(row).enumerate().for_each(|(y, drow)| {
                let srow = &src[y * row..(y + 1) * row];
                for x in 0..w {
                    let i = x * 3;
                    let ri = x.saturating_sub(d).min(w - 1) * 3;
                    let bi = (x + d).min(w - 1) * 3;
                    drow[i] = srow[ri];
                    drow[i + 2] = srow[bi + 2];
                }
            });
        }
    }
    if scanline > 0.01 {
        buf.par_chunks_mut(row).enumerate().for_each(|(y, drow)| {
            let gain = if y % 2 == 1 { 1.0 - scanline * 0.5 } else { 1.0 - scanline * 0.12 };
            for px in drow.chunks_mut(3) {
                px[0] = (px[0] as f64 * gain) as u8;
                px[1] = (px[1] as f64 * gain) as u8;
                px[2] = (px[2] as f64 * gain) as u8;
            }
        });
    }
}

/// S5 补充:荧光粉余辉。当前帧与上一帧混合,prev 为空(首帧)则原样返回。
/// k=persistence 强度 0-1,越大拖影越长。
pub fn phosphor(buf: &mut [u8], prev: &[u8], k: f64) {
    if prev.len() != buf.len() {
        return;
    }
    let keep = (k * 0.65).clamp(0.0, 0.85); // 上限防完全糊死
    for (i, b) in buf.iter_mut().enumerate() {
        *b = (((*b as f64) * (1.0 - keep)) + ((prev[i] as f64) * keep)) as u8;
    }
}

/// S6:胶片机械损伤。flicker=明暗抽风,scratches=竖向划痕,dust=尘埃噪点
pub fn film_damage(
    buf: &mut [u8],
    w: usize,
    h: usize,
    seed: u64,
    frame: usize,
    scratches: f64,
    dust: f64,
    flicker: f64,
) {
    let mut rng = Xorshift::new(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (frame as u64).wrapping_mul(1013));
    if flicker > 0.01 {
        let mult = 1.0 + (rng.f() - 0.5) * flicker * 0.18;
        // 带宽型单遍乘法:串行即可,rayon 分片反而亏
        for b in buf.iter_mut() {
            *b = ((*b as f64) * mult).clamp(0.0, 255.0) as u8;
        }
    }
    let n_scratch = (rng.f() * scratches * 3.0) as usize;
    for _ in 0..n_scratch {
        let x = rng.range(0, w);
        let y0 = rng.range(0, h / 2);
        let y1 = rng.range(y0 + h / 6, h);
        let bright = 40.0 + rng.f() * 80.0;
        for y in y0..y1 {
            for dx in 0..2 {
                let xx = (x + dx).min(w - 1);
                let i = (y * w + xx) * 3;
                for c in 0..3 {
                    buf[i + c] = (buf[i + c] as f64 + bright * 0.6).min(255.0) as u8;
                }
            }
        }
    }
    let n_dust = (rng.f() * dust * 60.0) as usize;
    for _ in 0..n_dust {
        let x = rng.range(0, w);
        let y = rng.range(0, h);
        let r = rng.range(1, 3);
        let white = rng.f() > 0.35;
        for dy in 0..r {
            for dx in 0..r {
                let i = ((y + dy).min(h - 1) * w + (x + dx).min(w - 1)) * 3;
                for c in 0..3 {
                    buf[i + c] = if white { buf[i + c].saturating_add(120) } else { buf[i + c].saturating_sub(120) };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_deterministic_per_seed_frame() {
        let mut a = Xorshift::new(7 ^ (3u64 * 1013));
        let mut b = Xorshift::new(7 ^ (3u64 * 1013));
        assert_eq!(a.f(), b.f());
    }

    #[test]
    fn crt_scanline_darkens_odd_rows() {
        let (w, h) = (8, 4);
        let mut buf = vec![200u8; w * h * 3];
        crt(&mut buf, w, h, 1.0, 0.0, 0.0);
        let even = buf[(0 * w + 0) * 3] as i32;
        let odd = buf[(1 * w + 0) * 3] as i32;
        assert!(odd < even, "奇数行应更暗: even={even} odd={odd}");
    }

    #[test]
    fn phosphor_blends_previous_frame() {
        let (w, h) = (4, 4);
        let mut cur = vec![0u8; w * h * 3]; // 全黑当前帧
        let prev = vec![200u8; w * h * 3]; // 全亮上一帧
        phosphor(&mut cur, &prev, 1.0);
        let v = cur[0];
        assert!(v > 60 && v < 185, "拖影应把黑帧抬到中间值,实得 {v}");
        // 首帧(prev 长度不符)保持不变
        let mut cur2 = vec![10u8; w * h * 3];
        let keep = cur2.clone();
        phosphor(&mut cur2, &[], 1.0);
        assert_eq!(cur2, keep);
    }

    #[test]
    fn film_damage_reproducible() {
        let (w, h) = (32, 32);
        let mut a = vec![128u8; w * h * 3];
        let mut b = vec![128u8; w * h * 3];
        film_damage(&mut a, w, h, 5, 1, 1.0, 1.0, 1.0);
        film_damage(&mut b, w, h, 5, 1, 1.0, 1.0, 1.0);
        assert_eq!(a, b);
        let mut c = vec![128u8; w * h * 3];
        film_damage(&mut c, w, h, 5, 2, 1.0, 1.0, 1.0);
        assert_ne!(a, c, "不同帧号应产生不同损伤");
    }
}
