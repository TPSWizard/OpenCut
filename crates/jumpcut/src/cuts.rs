//! Finding jump cuts on a timeline and planning the bridge across each.
//!
//! A jump cut is two clips back to back on one track, from the same file,
//! where the second does not carry on from where the first stopped in the
//! file (words or a pause were cut out, or a retake goes back). A plain
//! split that carries on is not one, nor a cut to another file.

/// One clip as the editors describe it: times in seconds, the source times
/// the clip shows at its start and end (after any speed change).
#[derive(Clone, Debug, PartialEq)]
pub struct CutClip {
    pub id: String,
    pub track: String,
    pub media: String,
    pub start: f64,
    pub end: f64,
    pub source_start: f64,
    pub source_end: f64,
}

/// A jump cut between `clips[before]` and `clips[after]` at `time`;
/// `source_gap` is how far the file jumps (negative: back).
#[derive(Clone, Debug, PartialEq)]
pub struct JumpCut {
    pub before: usize,
    pub after: usize,
    pub time: f64,
    pub source_gap: f64,
}

/// The jump cuts among `clips`, track by track (tracks in the order they
/// first appear), in time order. Clips are back to back, and the file
/// jumps, when the difference is at least half a frame at `fps`.
pub fn find_jump_cuts(clips: &[CutClip], fps: f64) -> Vec<JumpCut> {
    let half = 0.5 / fps.max(1e-6);
    let mut tracks: Vec<(&str, Vec<usize>)> = Vec::new();
    for (index, clip) in clips.iter().enumerate() {
        match tracks.iter_mut().find(|(track, _)| *track == clip.track) {
            Some((_, list)) => list.push(index),
            None => tracks.push((&clip.track, vec![index])),
        }
    }
    let mut cuts = Vec::new();
    for (_, mut list) in tracks {
        list.sort_by(|a, b| clips[*a].start.total_cmp(&clips[*b].start));
        for pair in list.windows(2) {
            let (a, b) = (&clips[pair[0]], &clips[pair[1]]);
            if (b.start - a.end).abs() > half || a.media != b.media {
                continue;
            }
            let gap = b.source_start - a.source_end;
            if gap.abs() < half {
                continue;
            }
            cuts.push(JumpCut { before: pair[0], after: pair[1], time: b.start, source_gap: gap });
        }
    }
    cuts
}

/// The bridge's frame counts the editors offer, and the default.
pub const BRIDGE_FRAMES: [usize; 3] = [4, 6, 10];
pub const BRIDGE_DEFAULT_FRAMES: usize = 6;

/// The bridge across a cut: `frames` in-between frames from `start`
/// (`frames / fps` long), blending from the first clip's frame shown at
/// `from_time` (the last one before the bridge) to the second clip's frame
/// at `to_time` (the first one after it); frame `i` is `alphas[i]` of the
/// way there.
#[derive(Clone, Debug, PartialEq)]
pub struct BridgePlan {
    pub start: f64,
    pub frames: usize,
    pub duration: f64,
    pub from_time: f64,
    pub to_time: f64,
    pub alphas: Vec<f64>,
}

/// Plans `frames` in-between frames across a cut at `cut` between a clip
/// starting at `before_start` and one ending at `after_end` (seconds):
/// half before the cut, half after (one more after when odd), each clip
/// keeping at least one frame of its own outside the bridge. Fewer frames
/// when the clips are short, down to two; `None` when not even two fit.
pub fn plan_bridge(cut: f64, before_start: f64, after_end: f64, fps: f64, frames: usize) -> Option<BridgePlan> {
    const EPS: f64 = 1e-6;
    let mut n = frames;
    while n >= 2 {
        let before = n / 2;
        let after = n - before;
        if cut - before_start >= (before + 1) as f64 / fps - EPS && after_end - cut >= (after + 1) as f64 / fps - EPS {
            return Some(BridgePlan {
                start: cut - before as f64 / fps,
                frames: n,
                duration: n as f64 / fps,
                from_time: cut - (before + 1) as f64 / fps,
                to_time: cut + after as f64 / fps,
                alphas: (0..n).map(|i| (i + 1) as f64 / (n + 1) as f64).collect(),
            });
        }
        n -= 1;
    }
    None
}
