//! In-between frames across a cut.
//!
//! The motion from the frame before the cut (P) to the frame after it (Q)
//! is measured by symmetric Lucas–Kanade on grey pictures, coarse to fine:
//! for each pixel `x` of the halfway picture, the motion `m` such that
//! `P(x − m/2)` matches `Q(x + m/2)` over a 7 × 7 window. The picture is
//! first averaged down so its longer side is at most [`WORK_MAX`] pixels,
//! then halved into up to [`MAX_LEVELS`] levels while the shorter side stays
//! at least [`LEVEL_MIN`]. Each level starts from the coarser one's motion
//! (doubled) and takes [`ITERATIONS`] solving steps (each at most a pixel),
//! each followed by smoothing over a 7 × 7 window in which every pixel
//! counts by how well its window pins the motion down (the smaller
//! eigenvalue of its gradients), so flat patches and straight edges take
//! the motion of the textured places round them. A frame `α` of the way
//! from P to Q is then `(1 − α)·P(x − α·m) + α·Q(x + (1 − α)·m)` for every
//! channel, with the motion scaled back up to the full picture.
//!
//! The grey value is the mean of the first three channels, so RGBA and
//! BGRA pictures give the same motion. Sampling is bilinear with edges
//! held. Big moves of fine repeating texture can be matched to the wrong
//! repeat (as with any motion estimate); a head turning or something
//! appearing has no motion to follow and comes out as a crossfade.

/// Longest side, in pixels, the motion is worked out at.
pub const WORK_MAX: usize = 320;
/// Shortest side a pyramid level may have.
pub const LEVEL_MIN: usize = 12;
pub const MAX_LEVELS: usize = 6;
const WINDOW_R: usize = 3;
pub const ITERATIONS: usize = 8;
const LAMBDA: f64 = 1e-4;
const SMOOTH_R: usize = 3;
/// Every pixel's least weight in the smoothing.
const CONFIDENCE_FLOOR: f64 = 1e-5;
const STEP_MAX: f64 = 1.0;

/// The motion from P to Q at the working size (`scale` times smaller than
/// the pictures), pixels of that size, for each pixel of the halfway frame.
#[derive(Clone, Debug, PartialEq)]
pub struct MotionField {
    pub width: usize,
    pub height: usize,
    pub scale: usize,
    pub u: Vec<f64>,
    pub v: Vec<f64>,
}

impl MotionField {
    /// The motion at a pixel of the full picture, in its pixels.
    pub fn at_full(&self, x: f64, y: f64) -> (f64, f64) {
        let s = self.scale as f64;
        let (sx, sy) = ((x + 0.5) / s - 0.5, (y + 0.5) / s - 0.5);
        (s * bilinear(&self.u, self.width, self.height, sx, sy), s * bilinear(&self.v, self.width, self.height, sx, sy))
    }

    /// The median motion, in full-picture pixels (the edges, where new
    /// picture comes in, can have outliers).
    pub fn median(&self) -> f64 {
        let mut lengths: Vec<f64> = self.u.iter().zip(&self.v).map(|(u, v)| u.hypot(*v)).collect();
        if lengths.is_empty() {
            return 0.0;
        }
        lengths.sort_by(f64::total_cmp);
        lengths[lengths.len() / 2] * self.scale as f64
    }
}

fn bilinear(img: &[f64], w: usize, h: usize, x: f64, y: f64) -> f64 {
    let x = x.clamp(0.0, (w - 1) as f64);
    let y = y.clamp(0.0, (h - 1) as f64);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f64, y - y0 as f64);
    let top = img[y0 * w + x0] * (1.0 - fx) + img[y0 * w + x1] * fx;
    let bottom = img[y1 * w + x0] * (1.0 - fx) + img[y1 * w + x1] * fx;
    top * (1.0 - fy) + bottom * fy
}

fn box_down(img: &[f64], w: usize, h: usize, s: usize) -> (Vec<f64>, usize, usize) {
    let (ww, wh) = (w / s, h / s);
    let inv = 1.0 / (s * s) as f64;
    let mut out = vec![0.0; ww * wh];
    for y in 0..wh {
        for x in 0..ww {
            let mut total = 0.0;
            for j in 0..s {
                let row = (y * s + j) * w + x * s;
                for i in 0..s {
                    total += img[row + i];
                }
            }
            out[y * ww + x] = total * inv;
        }
    }
    (out, ww, wh)
}

/// Sums over the in-bounds window of radius `r` round each pixel.
fn box_sum(img: &[f64], w: usize, h: usize, r: usize) -> Vec<f64> {
    let mut horiz = vec![0.0; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut total = 0.0;
            for i in x.saturating_sub(r)..=(x + r).min(w - 1) {
                total += img[y * w + i];
            }
            horiz[y * w + x] = total;
        }
    }
    let mut out = vec![0.0; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut total = 0.0;
            for j in y.saturating_sub(r)..=(y + r).min(h - 1) {
                total += horiz[j * w + x];
            }
            out[y * w + x] = total;
        }
    }
    out
}

fn window_count(w: usize, h: usize, x: usize, y: usize, r: usize) -> f64 {
    (((x + r).min(w - 1) - x.saturating_sub(r) + 1) * ((y + r).min(h - 1) - y.saturating_sub(r) + 1)) as f64
}

fn gradients(img: &[f64], w: usize, h: usize) -> (Vec<f64>, Vec<f64>) {
    let mut gx = vec![0.0; w * h];
    let mut gy = vec![0.0; w * h];
    for y in 0..h {
        for x in 0..w {
            gx[y * w + x] = (img[y * w + (x + 1).min(w - 1)] - img[y * w + x.saturating_sub(1)]) / 2.0;
            gy[y * w + x] = (img[(y + 1).min(h - 1) * w + x] - img[y.saturating_sub(1) * w + x]) / 2.0;
        }
    }
    (gx, gy)
}

fn grey(pixels: &[u8], w: usize, h: usize) -> Vec<f64> {
    (0..w * h)
        .map(|i| (f64::from(pixels[i * 4]) + f64::from(pixels[i * 4 + 1]) + f64::from(pixels[i * 4 + 2])) / (3.0 * 255.0))
        .collect()
}

/// The motion from `p` to `q`, both `width × height` four-channel pictures
/// (RGBA or BGRA). `None` when the sizes do not match the buffers or the
/// picture is under 2 × 2.
pub fn motion_between(p: &[u8], q: &[u8], width: usize, height: usize) -> Option<MotionField> {
    if width < 2 || height < 2 || p.len() != width * height * 4 || q.len() != p.len() {
        return None;
    }
    let s = width.max(height).div_ceil(WORK_MAX).max(1);
    let (p0, ww, wh) = box_down(&grey(p, width, height), width, height, s);
    let (q0, _, _) = box_down(&grey(q, width, height), width, height, s);
    if ww < 2 || wh < 2 {
        return None;
    }
    let mut levels = vec![(p0, q0, ww, wh)];
    while levels.len() < MAX_LEVELS {
        let (p, q, lw, lh) = levels.last().unwrap();
        if (lw / 2).min(lh / 2) < LEVEL_MIN {
            break;
        }
        let (pn, nw, nh) = box_down(p, *lw, *lh, 2);
        let (qn, _, _) = box_down(q, *lw, *lh, 2);
        levels.push((pn, qn, nw, nh));
    }

    let mut u: Vec<f64> = Vec::new();
    let mut v: Vec<f64> = Vec::new();
    let mut previous = (0, 0);
    for (p, q, lw, lh) in levels.iter().rev() {
        let (lw, lh) = (*lw, *lh);
        let n = lw * lh;
        if u.is_empty() {
            u = vec![0.0; n];
            v = vec![0.0; n];
        } else {
            let (cw, ch) = previous;
            let mut nu = vec![0.0; n];
            let mut nv = vec![0.0; n];
            for y in 0..lh {
                for x in 0..lw {
                    let (sx, sy) = ((x as f64 + 0.5) / 2.0 - 0.5, (y as f64 + 0.5) / 2.0 - 0.5);
                    nu[y * lw + x] = 2.0 * bilinear(&u, cw, ch, sx, sy);
                    nv[y * lw + x] = 2.0 * bilinear(&v, cw, ch, sx, sy);
                }
            }
            u = nu;
            v = nv;
        }
        previous = (lw, lh);
        let (pgx, pgy) = gradients(p, lw, lh);
        let (qgx, qgy) = gradients(q, lw, lh);
        let (mut xx, mut xy, mut yy, mut xr, mut yr) = (vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]);
        let mut confidence = vec![0.0; n];
        for _ in 0..ITERATIONS {
            for y in 0..lh {
                for x in 0..lw {
                    let k = y * lw + x;
                    let (ax, ay) = (x as f64 - u[k] / 2.0, y as f64 - v[k] / 2.0);
                    let (bx, by) = (x as f64 + u[k] / 2.0, y as f64 + v[k] / 2.0);
                    let r = bilinear(q, lw, lh, bx, by) - bilinear(p, lw, lh, ax, ay);
                    let gx = (bilinear(&pgx, lw, lh, ax, ay) + bilinear(&qgx, lw, lh, bx, by)) / 2.0;
                    let gy = (bilinear(&pgy, lw, lh, ax, ay) + bilinear(&qgy, lw, lh, bx, by)) / 2.0;
                    xx[k] = gx * gx;
                    xy[k] = gx * gy;
                    yy[k] = gy * gy;
                    xr[k] = gx * r;
                    yr[k] = gy * r;
                }
            }
            let (sxx, sxy, syy, sxr, syr) = (
                box_sum(&xx, lw, lh, WINDOW_R),
                box_sum(&xy, lw, lh, WINDOW_R),
                box_sum(&yy, lw, lh, WINDOW_R),
                box_sum(&xr, lw, lh, WINDOW_R),
                box_sum(&yr, lw, lh, WINDOW_R),
            );
            for y in 0..lh {
                for x in 0..lw {
                    let k = y * lw + x;
                    let count = window_count(lw, lh, x, y, WINDOW_R);
                    // The smaller eigenvalue of the window's structure: how
                    // well the motion is pinned down here (none on a flat
                    // patch or along a straight edge).
                    let half_trace = (sxx[k] + syy[k]) / 2.0;
                    let spread = (((sxx[k] - syy[k]) / 2.0).powi(2) + sxy[k] * sxy[k]).sqrt();
                    confidence[k] = (half_trace - spread).max(0.0) / count;
                    let lambda = LAMBDA * count;
                    let (a, b, d) = (sxx[k] + lambda, sxy[k], syy[k] + lambda);
                    let det = a * d - b * b;
                    let mut du = -(d * sxr[k] - b * syr[k]) / det;
                    let mut dv = -(a * syr[k] - b * sxr[k]) / det;
                    let length = du.hypot(dv);
                    if length > STEP_MAX {
                        du *= STEP_MAX / length;
                        dv *= STEP_MAX / length;
                    }
                    u[k] += du;
                    v[k] += dv;
                }
            }
            // Smooth, each pixel weighted by its confidence, so flat
            // patches take the motion of the textured ones round them.
            let weighted_u: Vec<f64> = (0..n).map(|k| confidence[k] * u[k]).collect();
            let weighted_v: Vec<f64> = (0..n).map(|k| confidence[k] * v[k]).collect();
            let (cu, cv, c) = (
                box_sum(&weighted_u, lw, lh, SMOOTH_R),
                box_sum(&weighted_v, lw, lh, SMOOTH_R),
                box_sum(&confidence, lw, lh, SMOOTH_R),
            );
            let (su, sv) = (box_sum(&u, lw, lh, SMOOTH_R), box_sum(&v, lw, lh, SMOOTH_R));
            for y in 0..lh {
                for x in 0..lw {
                    let k = y * lw + x;
                    let count = window_count(lw, lh, x, y, SMOOTH_R);
                    u[k] = (cu[k] + CONFIDENCE_FLOOR * su[k]) / (c[k] + CONFIDENCE_FLOOR * count);
                    v[k] = (cv[k] + CONFIDENCE_FLOOR * sv[k]) / (c[k] + CONFIDENCE_FLOOR * count);
                }
            }
        }
    }
    Some(MotionField { width: ww, height: wh, scale: s, u, v })
}

/// The frame `alpha` (0 to 1) of the way from `p` to `q` along `field`;
/// same layout as the inputs, every channel (alpha too) blended.
pub fn in_between(p: &[u8], q: &[u8], width: usize, height: usize, field: &MotionField, alpha: f64) -> Vec<u8> {
    let (w, h) = (width, height);
    let mut out = vec![0u8; w * h * 4];
    if p.len() != w * h * 4 || q.len() != p.len() || w == 0 || h == 0 {
        return out;
    }
    let alpha = alpha.clamp(0.0, 1.0);
    let sample = |img: &[u8], x: f64, y: f64| -> [f64; 4] {
        let x = x.clamp(0.0, (w - 1) as f64);
        let y = y.clamp(0.0, (h - 1) as f64);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (fx, fy) = (x - x0 as f64, y - y0 as f64);
        let mut result = [0.0; 4];
        for (c, value) in result.iter_mut().enumerate() {
            let at = |xx: usize, yy: usize| f64::from(img[(yy * w + xx) * 4 + c]);
            let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
            let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
            *value = top * (1.0 - fy) + bottom * fy;
        }
        result
    };
    for y in 0..h {
        for x in 0..w {
            let (mu, mv) = field.at_full(x as f64, y as f64);
            let a = sample(p, x as f64 - alpha * mu, y as f64 - alpha * mv);
            let b = sample(q, x as f64 + (1.0 - alpha) * mu, y as f64 + (1.0 - alpha) * mv);
            for c in 0..4 {
                let value = (1.0 - alpha) * a[c] + alpha * b[c];
                out[(y * w + x) * 4 + c] = (value + 0.5).floor().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}
