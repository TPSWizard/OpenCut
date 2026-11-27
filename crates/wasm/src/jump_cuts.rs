//! Smooth jump cuts (row 106) for the web editor: the shared `jumpcut`
//! crate finds the cuts, plans each bridge and makes its in-between frames,
//! as the desktop editor does.

use wasm_bindgen::prelude::*;

/// The bridge lengths offered (frames) and, last, the default.
#[wasm_bindgen(js_name = jumpCutFrameChoices)]
pub fn jump_cut_frame_choices() -> Vec<u32> {
    let mut choices: Vec<u32> = jumpcut::BRIDGE_FRAMES.iter().map(|frames| *frames as u32).collect();
    choices.push(jumpcut::BRIDGE_DEFAULT_FRAMES as u32);
    choices
}

/// The jump cuts among clips given flat, six numbers each: track index,
/// media index, start, end, source start, source end (seconds). Per cut,
/// flat: the clips' indexes before and after, the cut time and how far
/// the file jumps (`jumpcut::find_jump_cuts`).
#[wasm_bindgen(js_name = jumpCutFind)]
pub fn jump_cut_find(clips: &[f64], fps: f64) -> Vec<f64> {
    let clips: Vec<jumpcut::CutClip> = clips
        .chunks_exact(6)
        .enumerate()
        .map(|(index, clip)| jumpcut::CutClip {
            id: index.to_string(),
            track: format!("{}", clip[0]),
            media: format!("{}", clip[1]),
            start: clip[2],
            end: clip[3],
            source_start: clip[4],
            source_end: clip[5],
        })
        .collect();
    jumpcut::find_jump_cuts(&clips, fps)
        .into_iter()
        .flat_map(|cut| [cut.before as f64, cut.after as f64, cut.time, cut.source_gap])
        .collect()
}

/// The bridge across a cut (`jumpcut::plan_bridge`): `start, frames,
/// duration, fromTime, toTime`, then each frame's alpha; empty when not
/// even two frames fit.
#[wasm_bindgen(js_name = jumpCutPlan)]
pub fn jump_cut_plan(cut: f64, before_start: f64, after_end: f64, fps: f64, frames: u32) -> Vec<f64> {
    match jumpcut::plan_bridge(cut, before_start, after_end, fps, frames as usize) {
        None => Vec::new(),
        Some(plan) => {
            let mut out = vec![plan.start, plan.frames as f64, plan.duration, plan.from_time, plan.to_time];
            out.extend(plan.alphas);
            out
        }
    }
}

/// The motion between two RGBA pictures (`jumpcut::motion_between`), flat:
/// work width, work height, scale, median motion (pixels), then the `u`
/// and `v` planes. Empty when the pictures do not fit their size.
#[wasm_bindgen(js_name = jumpCutMotion)]
pub fn jump_cut_motion(p: &[u8], q: &[u8], width: u32, height: u32) -> Vec<f64> {
    match jumpcut::motion_between(p, q, width as usize, height as usize) {
        None => Vec::new(),
        Some(field) => {
            let mut out = vec![field.width as f64, field.height as f64, field.scale as f64, field.median()];
            out.extend(&field.u);
            out.extend(&field.v);
            out
        }
    }
}

fn field_from(motion: &[f64]) -> Option<jumpcut::MotionField> {
    if motion.len() < 4 {
        return None;
    }
    let (width, height, scale) = (motion[0] as usize, motion[1] as usize, motion[2] as usize);
    let n = width * height;
    if motion.len() != 4 + 2 * n || scale == 0 {
        return None;
    }
    Some(jumpcut::MotionField { width, height, scale, u: motion[4..4 + n].to_vec(), v: motion[4 + n..].to_vec() })
}

/// The RGBA frame `alpha` of the way from `p` to `q` along `motion` (as
/// [`jump_cut_motion`] returned it); empty when the motion does not fit.
#[wasm_bindgen(js_name = jumpCutFrame)]
pub fn jump_cut_frame(p: &[u8], q: &[u8], width: u32, height: u32, motion: &[f64], alpha: f64) -> Vec<u8> {
    match field_from(motion) {
        None => Vec::new(),
        Some(field) => jumpcut::in_between(p, q, width as usize, height as usize, &field, alpha),
    }
}
