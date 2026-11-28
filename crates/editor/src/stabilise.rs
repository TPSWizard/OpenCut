//! Stabilisation on a video clip (row 107): the `stabilise.strength` param
//! (off, light, medium, strong, still) and the clip's measured camera path
//! in `stabilise.path` (`stabilise::CameraPath`, tagged with the media it
//! was measured from, so a replaced file is not corrected with an old
//! path). The planner turns them into the frame's correction at its source
//! time, which the renderer adds to the clip's transform (moved, turned and
//! zoomed so the camera holds steady). The web editor reads the same params
//! (`apps/web/src/stabilise`).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

pub use stabilise::Strength as StabiliseStrength;
use stabilise::{CameraPath, Corrections, Similarity, Strength};

use crate::{LayerTransform, Params};

pub const STABILISE_STRENGTH: &str = "stabilise.strength";
pub const STABILISE_PATH: &str = "stabilise.path";

/// The clip's stabilise strength (off when unset or unknown).
pub fn read_stabilise_strength(params: &Params) -> Strength {
    Strength::parse(&params.text(STABILISE_STRENGTH, "off")).unwrap_or(Strength::Off)
}

/// The clip's measured camera path, when it was measured from `media_id`.
pub fn read_stabilise_path(params: &Params, media_id: &str) -> Option<CameraPath> {
    CameraPath::parse(&params.text(STABILISE_PATH, "")).filter(|path| path.media_id == media_id)
}

/// Corrections by path text and strength; a few clips' worth, as every frame asks.
type Cached = (u64, Strength, Arc<Corrections>);
static CACHE: OnceLock<Mutex<Vec<Cached>>> = OnceLock::new();
const CACHE_KEEP: usize = 8;

/// The clip's corrections for its strength, or `None` when off or unmeasured.
pub fn stabilise_corrections(params: &Params, media_id: &str) -> Option<Arc<Corrections>> {
    let strength = read_stabilise_strength(params);
    if strength == Strength::Off {
        return None;
    }
    let text = params.text(STABILISE_PATH, "");
    // The path names the media it was measured from first.
    if text.split_once(';').map(|(media, _)| media) != Some(media_id) {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    let key = hasher.finish();
    let cache = CACHE.get_or_init(Default::default);
    if let Some((_, _, found)) = cache.lock().unwrap().iter().find(|(k, s, _)| *k == key && *s == strength) {
        return Some(found.clone());
    }
    let path = CameraPath::parse(&text).filter(|path| path.media_id == media_id)?;
    let corrections = Arc::new(path.corrections(strength));
    let mut cache = cache.lock().unwrap();
    cache.push((key, strength, corrections.clone()));
    if cache.len() > CACHE_KEEP {
        cache.remove(0);
    }
    Some(corrections)
}

/// The frame's correction at `source_seconds`, or `None` when stabilise is
/// off, unmeasured, or leaves the frame as it is.
pub fn stabilise_correction(params: &Params, media_id: &str, source_seconds: f64) -> Option<Similarity> {
    let correction = stabilise_corrections(params, media_id)?.at(source_seconds);
    let moved = (correction.a - 1.0).abs() > 1e-9 || correction.b.abs() > 1e-9 || correction.tx.abs() > 1e-9 || correction.ty.abs() > 1e-9;
    moved.then_some(correction)
}

/// `transform` with the frame moved by `correction`, for a frame drawn
/// `base_width` canvas pixels wide at scale 1 (the web app's `steadyTransform`).
pub fn steady_transform(transform: LayerTransform, correction: &Similarity, base_width: f64) -> LayerTransform {
    let (local_x, local_y, turn, scale) = stabilise::drawn_offset(
        correction,
        base_width * transform.scale_x.abs() as f64,
        base_width * transform.scale_y.abs() as f64,
        transform.scale_x < 0.0,
        transform.scale_y < 0.0,
    );
    let (sin, cos) = (transform.rotate_degrees as f64).to_radians().sin_cos();
    LayerTransform {
        position_x: transform.position_x + (local_x * cos - local_y * sin) as f32,
        position_y: transform.position_y + (local_x * sin + local_y * cos) as f32,
        scale_x: transform.scale_x * scale as f32,
        scale_y: transform.scale_y * scale as f32,
        rotate_degrees: transform.rotate_degrees + turn as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrections_need_a_strength_and_a_path_from_this_media() {
        let shaky = Similarity { a: 1.0, b: 0.0, tx: 0.01, ty: 0.0 };
        let path = CameraPath {
            media_id: "m".into(),
            start: 0.0,
            rate: 10.0,
            aspect: 0.5625,
            steps: vec![Some(shaky), Some(shaky.inverse()), Some(shaky), Some(shaky.inverse())],
        };
        let mut params = Params::new();
        params.set(STABILISE_PATH, path.encode());
        assert_eq!(stabilise_correction(&params, "m", 0.1), None, "off by default");
        params.set(STABILISE_STRENGTH, "still");
        let correction = stabilise_correction(&params, "m", 0.1).expect("the jolted frame is moved back");
        // The picture jumped right in frame 1, so it is moved back left.
        assert!(correction.tx < 0.0, "{correction:?}");
        assert_eq!(stabilise_correction(&params, "other", 0.1), None, "a path from another file is ignored");
        params.set(STABILISE_STRENGTH, "bogus");
        assert_eq!(read_stabilise_strength(&params), Strength::Off);
    }

    #[test]
    fn the_offset_follows_the_clip_turn() {
        let transform = LayerTransform { position_x: 10.0, position_y: 0.0, scale_x: 0.5, scale_y: 0.5, rotate_degrees: 90.0 };
        let correction = Similarity { a: 1.1, b: 0.0, tx: 0.1, ty: 0.0 };
        let moved = steady_transform(transform, &correction, 1000.0);
        // A tenth of the 500 px drawn frame along its own x, which points down once turned 90°.
        assert!((moved.position_x - 10.0).abs() < 1e-3 && (moved.position_y - 50.0).abs() < 1e-3, "{moved:?}");
        assert!((moved.scale_x - 0.55).abs() < 1e-6 && moved.rotate_degrees == 90.0);
    }
}
