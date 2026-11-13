//! Picture cleanup for cut-out illustrations (deterministic, no models):
//! remove a flat white background by flood fill from the border with a
//! soft edge, take the white back out of the edge pixels (halo removal),
//! feather the edge inwards, tint the ink, crop to the content, and trace
//! the shape's longest skeleton path (where a light pulse can travel). The
//! operations and the `TracedPath` shape follow the Shortform Studio's
//! `lib/shortform/imageops.ts` (overtime-ai-studio), so paths traced here
//! and there are interchangeable. The web app's `src/picture/cleanup.ts` is
//! the same arithmetic.

use serde::{Deserialize, Serialize};

/// Straight-alpha RGBA, row by row.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// A polyline along the shape, points as fractions of the picture (0..1,
/// three decimals), top end first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TracedPath {
    pub id: String,
    pub points: Vec<[f64; 2]>,
}

/// What a cleanup does; see [`clean_picture`].
#[derive(Clone, Debug, PartialEq)]
pub struct PictureCleanup {
    pub remove_white: bool,
    pub decontaminate: bool,
    /// Edge softening in pixels (0 = none); only ever lowers alpha.
    pub feather: usize,
    /// Ink colour `#rrggbb`, or none to keep the picture's colours.
    pub tint: Option<String>,
    pub crop: bool,
    pub trace: bool,
}

impl Default for PictureCleanup {
    fn default() -> Self {
        Self { remove_white: true, decontaminate: true, feather: 1, tint: None, crop: true, trace: true }
    }
}

fn lum(rgba: &[u8], i: usize) -> f64 {
    (0.2126 * rgba[i] as f64 + 0.7152 * rgba[i + 1] as f64 + 0.0722 * rgba[i + 2] as f64) / 255.0
}

/// Flood-fills near-white pixels connected to the border and makes them
/// transparent; pixels between `soft` and `hard` luminance fade.
pub fn remove_flat_white(picture: &mut Picture, hard: f64, soft: f64) {
    let (w, h) = (picture.width, picture.height);
    if w == 0 || h == 0 {
        return;
    }
    let d = &mut picture.rgba;
    let mut seen = vec![false; w * h];
    let mut stack: Vec<usize> = Vec::new();
    let push = |p: usize, d: &Vec<u8>, seen: &mut Vec<bool>, stack: &mut Vec<usize>| {
        if seen[p] || lum(d, p * 4) < soft {
            return;
        }
        seen[p] = true;
        stack.push(p);
    };
    for x in 0..w {
        push(x, d, &mut seen, &mut stack);
        push((h - 1) * w + x, d, &mut seen, &mut stack);
    }
    for y in 0..h {
        push(y * w, d, &mut seen, &mut stack);
        push(y * w + w - 1, d, &mut seen, &mut stack);
    }
    while let Some(p) = stack.pop() {
        let l = lum(d, p * 4);
        let a = if l >= hard { 0.0 } else { (255.0 * (1.0 - (l - soft) / (hard - soft))).round() };
        d[p * 4 + 3] = d[p * 4 + 3].min(a.clamp(0.0, 255.0) as u8);
        if l < hard {
            continue;
        }
        let (x, y) = (p % w, p / w);
        if x > 0 {
            push(p - 1, d, &mut seen, &mut stack);
        }
        if x + 1 < w {
            push(p + 1, d, &mut seen, &mut stack);
        }
        if y > 0 {
            push(p - w, d, &mut seen, &mut stack);
        }
        if y + 1 < h {
            push(p + w, d, &mut seen, &mut stack);
        }
    }
}

/// Halo removal: a soft edge pixel shows the ink mixed with the white it
/// was cut from (`c = a·ink + (1−a)·255`); this takes the white back out.
pub fn decontaminate_white(picture: &mut Picture) {
    for px in picture.rgba.chunks_exact_mut(4) {
        let a = px[3] as f64 / 255.0;
        if a <= 0.0 || a >= 1.0 {
            continue;
        }
        for c in px.iter_mut().take(3) {
            let ink = (*c as f64 - (1.0 - a) * 255.0) / a;
            *c = ink.round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Softens the edge inwards: alpha becomes the smaller of itself and its
/// `radius`-pixel box blur, so nothing outside the shape gains opacity.
pub fn feather_alpha(picture: &mut Picture, radius: usize) {
    let (w, h) = (picture.width, picture.height);
    if radius == 0 || w == 0 || h == 0 {
        return;
    }
    let alpha: Vec<f64> = picture.rgba.chunks_exact(4).map(|px| px[3] as f64).collect();
    let blur_line = |get: &dyn Fn(usize) -> f64, len: usize| -> Vec<f64> {
        (0..len)
            .map(|i| {
                let from = i.saturating_sub(radius);
                let to = (i + radius).min(len - 1);
                // Outside the picture counts as transparent.
                let sum: f64 = (from..=to).map(get).sum();
                sum / (2 * radius + 1) as f64
            })
            .collect()
    };
    let mut horizontal = vec![0.0; w * h];
    for y in 0..h {
        let row = blur_line(&|x| alpha[y * w + x], w);
        horizontal[y * w..(y + 1) * w].copy_from_slice(&row);
    }
    for x in 0..w {
        let column = blur_line(&|y| horizontal[y * w + x], h);
        for (y, value) in column.into_iter().enumerate() {
            let i = (y * w + x) * 4 + 3;
            picture.rgba[i] = picture.rgba[i].min(value.round().clamp(0.0, 255.0) as u8);
        }
    }
}

/// Ink tint: every pixel takes `hex`'s colour; alpha scales with darkness
/// (and keeps the cut-out's alpha). Pale paper turns transparent.
pub fn ink_tint(picture: &mut Picture, hex: &str, gain: f64) {
    let channel = |range: std::ops::Range<usize>| u8::from_str_radix(hex.get(range).unwrap_or("ff"), 16).unwrap_or(255);
    let (r, g, b) = (channel(1..3), channel(3..5), channel(5..7));
    for i in (0..picture.rgba.len()).step_by(4) {
        let dark = 1.0 - lum(&picture.rgba, i);
        let ink = ((dark - 0.02) * gain).clamp(0.0, 1.0);
        let a = picture.rgba[i + 3] as f64 / 255.0 * ink;
        picture.rgba[i] = r;
        picture.rgba[i + 1] = g;
        picture.rgba[i + 2] = b;
        picture.rgba[i + 3] = (a * 255.0).round() as u8;
    }
}

/// A tight crop around pixels more opaque than `alpha_min`, padded by
/// `pad_fraction` of the content's longer side.
pub fn crop_to_content(picture: &Picture, pad_fraction: f64, alpha_min: u8) -> Picture {
    let (w, h) = (picture.width, picture.height);
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
    let mut any = false;
    for y in 0..h {
        for x in 0..w {
            if picture.rgba[(y * w + x) * 4 + 3] > alpha_min {
                any = true;
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
    }
    if !any {
        return picture.clone();
    }
    let pad = ((x1 - x0).max(y1 - y0) as f64 * pad_fraction).round() as usize;
    let (x0, y0) = (x0.saturating_sub(pad), y0.saturating_sub(pad));
    let (x1, y1) = ((x1 + pad).min(w - 1), (y1 + pad).min(h - 1));
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    let mut rgba = Vec::with_capacity(cw * ch * 4);
    for y in 0..ch {
        let start = ((y0 + y) * w + x0) * 4;
        rgba.extend_from_slice(&picture.rgba[start..start + cw * 4]);
    }
    Picture { width: cw, height: ch, rgba }
}

/// Zhang–Suen thinning of a 0/1 mask, in place.
pub fn thin(m: &mut [u8], w: usize, h: usize) {
    let at = |m: &[u8], x: i64, y: i64| -> u8 {
        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 { 0 } else { m[y as usize * w + x as usize] }
    };
    let mut changed = true;
    let mut delete: Vec<usize> = Vec::new();
    while changed {
        changed = false;
        for pass in 0..2 {
            delete.clear();
            for y in 0..h as i64 {
                for x in 0..w as i64 {
                    if m[y as usize * w + x as usize] == 0 {
                        continue;
                    }
                    let p = [
                        at(m, x, y - 1),
                        at(m, x + 1, y - 1),
                        at(m, x + 1, y),
                        at(m, x + 1, y + 1),
                        at(m, x, y + 1),
                        at(m, x - 1, y + 1),
                        at(m, x - 1, y),
                        at(m, x - 1, y - 1),
                    ];
                    let b: u8 = p.iter().sum();
                    if !(2..=6).contains(&b) {
                        continue;
                    }
                    let a = (0..8).filter(|&k| p[k] == 0 && p[(k + 1) % 8] == 1).count();
                    if a != 1 {
                        continue;
                    }
                    let blocked = if pass == 0 {
                        p[0] * p[2] * p[4] != 0 || p[2] * p[4] * p[6] != 0
                    } else {
                        p[0] * p[2] * p[6] != 0 || p[0] * p[4] * p[6] != 0
                    };
                    if blocked {
                        continue;
                    }
                    delete.push(y as usize * w + x as usize);
                }
            }
            if !delete.is_empty() {
                changed = true;
            }
            for &q in &delete {
                m[q] = 0;
            }
        }
    }
}

/// Breadth-first search over the 8-connected mask: the last pixel reached
/// and each pixel's predecessor (-2 unreached, -1 the start).
fn bfs_farthest(m: &[u8], w: usize, h: usize, start: usize) -> (usize, Vec<i64>) {
    let mut prev = vec![-2i64; w * h];
    prev[start] = -1;
    let mut queue = vec![start];
    let mut far = start;
    let mut head = 0;
    while head < queue.len() {
        let p = queue[head];
        head += 1;
        far = p;
        let (x, y) = ((p % w) as i64, (p / w) as i64);
        for dy in -1..=1i64 {
            for dx in -1..=1i64 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                    continue;
                }
                let n = ny as usize * w + nx as usize;
                if m[n] != 0 && prev[n] == -2 {
                    prev[n] = p as i64;
                    queue.push(n);
                }
            }
        }
    }
    (far, prev)
}

fn simplify(points: &[[f64; 2]], tolerance: f64) -> Vec<[f64; 2]> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let (a, b) = (points[0], points[points.len() - 1]);
    let (mut worst, mut at) = (0.0, 0);
    for (i, p) in points.iter().enumerate().take(points.len() - 1).skip(1) {
        let num = ((b[1] - a[1]) * p[0] - (b[0] - a[0]) * p[1] + b[0] * a[1] - b[1] * a[0]).abs();
        let den = (b[1] - a[1]).hypot(b[0] - a[0]);
        let den = if den == 0.0 { 1.0 } else { den };
        if num / den > worst {
            worst = num / den;
            at = i;
        }
    }
    if worst <= tolerance {
        return vec![a, b];
    }
    let mut left = simplify(&points[..=at], tolerance);
    left.pop();
    left.extend(simplify(&points[at..], tolerance));
    left
}

/// The longest skeleton path of the opaque shape (alpha over 96), top end
/// first, as fractions of the picture; at most 64 points.
pub fn trace_main_path(picture: &Picture, id: &str) -> Option<TracedPath> {
    let (pw, ph) = (picture.width, picture.height);
    if pw == 0 || ph == 0 {
        return None;
    }
    let s = (220.0 / pw.max(ph) as f64).min(1.0);
    let w = ((pw as f64 * s).round() as usize).max(2);
    let h = ((ph as f64 * s).round() as usize).max(2);
    let mut m = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let ix = (((x as f64 + 0.5) / s).floor() as usize).min(pw - 1);
            let iy = (((y as f64 + 0.5) / s).floor() as usize).min(ph - 1);
            m[y * w + x] = u8::from(picture.rgba[(iy * pw + ix) * 4 + 3] > 96);
        }
    }
    let (sx, sy) = (pw as f64 / w as f64, ph as f64 / h as f64);
    thin(&mut m, w, h);
    let (mut seed, mut best, mut best_len) = (None, 0usize, 0usize);
    let mut visited = vec![false; w * h];
    for p in 0..m.len() {
        if m[p] == 0 || visited[p] {
            continue;
        }
        let (far, prev) = bfs_farthest(&m, w, h, p);
        for (i, v) in prev.iter().enumerate() {
            if *v != -2 {
                visited[i] = true;
            }
        }
        let (second_far, second_prev) = bfs_farthest(&m, w, h, far);
        let mut len = 0;
        let mut c = second_far as i64;
        while c != -1 {
            len += 1;
            c = second_prev[c as usize];
        }
        if len > best_len {
            best_len = len;
            seed = Some(far);
            best = second_far;
        }
    }
    let seed = seed?;
    if best_len < 4 {
        return None;
    }
    let (_, prev) = bfs_farthest(&m, w, h, seed);
    let mut raw: Vec<[f64; 2]> = Vec::new();
    let mut c = best as i64;
    while c >= 0 {
        let p = c as usize;
        raw.push([((p % w) as f64 + 0.5) * sx, ((p / w) as f64 + 0.5) * sy]);
        c = prev[p];
    }
    if raw.len() < 2 {
        return None;
    }
    if raw[0][1] > raw[raw.len() - 1][1] {
        raw.reverse();
    }
    let simple = simplify(&raw, pw.max(ph) as f64 * 0.006);
    let round = |v: f64| (v * 1000.0).round() / 1000.0;
    Some(TracedPath {
        id: id.to_owned(),
        points: simple.iter().take(64).map(|[x, y]| [round(x / pw as f64), round(y / ph as f64)]).collect(),
    })
}

/// The whole cleanup in order: white removal, halo removal, feather, tint,
/// crop, then tracing on the result.
pub fn clean_picture(picture: &Picture, options: &PictureCleanup) -> (Picture, Option<TracedPath>) {
    let mut out = picture.clone();
    if options.remove_white {
        remove_flat_white(&mut out, 0.93, 0.8);
    }
    if options.decontaminate {
        decontaminate_white(&mut out);
    }
    feather_alpha(&mut out, options.feather);
    if let Some(tint) = options.tint.as_deref().filter(|hex| hex.len() == 7 && hex.starts_with('#')) {
        ink_tint(&mut out, tint, 1.35);
    }
    if options.crop {
        out = crop_to_content(&out, 0.04, 12);
    }
    let path = if options.trace { trace_main_path(&out, "main") } else { None };
    (out, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic drawing: white paper, a dark grey stroke from top left to
    /// bottom right (a nerve fibre, three pixels thick) and a black dot,
    /// with a one-pixel light-grey (215) antialiased rim. Mirrored in the web app's
    /// `src/picture/__tests__/cleanup.test.ts`.
    pub(crate) fn drawing() -> Picture {
        let (w, h) = (64usize, 48usize);
        let mut rgba = vec![255u8; w * h * 4];
        let mut set = |x: usize, y: usize, v: u8| {
            let i = (y * w + x) * 4;
            if rgba[i] > v {
                rgba[i..i + 3].copy_from_slice(&[v, v, v]);
            }
        };
        for t in 0..=40 {
            let (cx, cy) = (10 + t, 6 + (t * 3) / 4);
            for dy in -2i64..=2 {
                for dx in -2i64..=2 {
                    let (x, y) = ((cx as i64 + dx) as usize, (cy as i64 + dy) as usize);
                    let edge = dx.abs() == 2 || dy.abs() == 2;
                    set(x, y, if edge { 215 } else { 60 });
                }
            }
        }
        for y in 34..40 {
            for x in 50..56 {
                set(x, y, 0);
            }
        }
        Picture { width: w, height: h, rgba }
    }

    #[test]
    fn white_paper_goes_and_the_ink_stays() {
        let mut picture = drawing();
        remove_flat_white(&mut picture, 0.93, 0.8);
        let alpha = |p: &Picture, x: usize, y: usize| p.rgba[(y * p.width + x) * 4 + 3];
        assert_eq!(alpha(&picture, 0, 0), 0, "paper");
        assert_eq!(alpha(&picture, 30, 21), 255, "stroke core");
        let rim = alpha(&picture, 10, 4);
        assert!(rim > 0 && rim < 255, "the 215-grey rim fades: {rim}");
        decontaminate_white(&mut picture);
        let i = (4 * picture.width + 10) * 4;
        assert!(picture.rgba[i] < 215, "the rim loses its white: {}", picture.rgba[i]);
    }

    #[test]
    fn feathering_only_softens() {
        let mut picture = drawing();
        remove_flat_white(&mut picture, 0.93, 0.8);
        let before: Vec<u8> = picture.rgba.chunks_exact(4).map(|p| p[3]).collect();
        feather_alpha(&mut picture, 2);
        let after: Vec<u8> = picture.rgba.chunks_exact(4).map(|p| p[3]).collect();
        assert!(before.iter().zip(&after).all(|(b, a)| a <= b));
        assert!(after.iter().zip(&before).any(|(a, b)| a < b));
    }

    #[test]
    fn tint_and_crop() {
        let mut picture = drawing();
        remove_flat_white(&mut picture, 0.93, 0.8);
        ink_tint(&mut picture, "#ffffff", 1.35);
        let i = (21 * picture.width + 30) * 4;
        assert_eq!(&picture.rgba[i..i + 3], &[255, 255, 255]);
        assert!(picture.rgba[i + 3] > 150, "dark ink stays opaque");
        let cropped = crop_to_content(&picture, 0.04, 12);
        assert!(cropped.width < picture.width && cropped.height < picture.height);
    }

    /// The same expectations are in `src/picture/__tests__/cleanup.test.ts`.
    #[test]
    fn matches_the_web_reference() {
        let (out, path) = clean_picture(&drawing(), &PictureCleanup { tint: Some("#ffffff".into()), feather: 1, ..Default::default() });
        let alpha_sum: u64 = out.rgba.chunks_exact(4).map(|p| p[3] as u64).sum();
        let path = path.expect("a path");
        assert_eq!((out.width, out.height, alpha_sum), (52, 40, 58_924));
        let expected: Vec<[f64; 2]> = vec![[0.087, 0.113], [0.106, 0.113], [0.163, 0.188], [0.183, 0.188], [0.24, 0.263], [0.26, 0.263], [0.317, 0.338], [0.337, 0.338], [0.394, 0.413], [0.413, 0.413], [0.471, 0.488], [0.49, 0.488], [0.548, 0.563], [0.567, 0.563], [0.625, 0.638], [0.644, 0.638], [0.702, 0.713], [0.721, 0.713], [0.779, 0.788], [0.798, 0.788], [0.837, 0.838], [0.875, 0.838], [0.894, 0.863]];
        assert_eq!(path.points, expected);
    }
}
