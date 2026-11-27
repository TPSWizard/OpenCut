//! Fit music to a length (row 101): shorten a track by cutting out bars,
//! or lengthen it by looping bars, at joins where the music sounds the
//! same either side, so the edit is hard to hear.
//!
//! 1. **Beats and bars.** The clip's sound (mono, any rate; both editors
//!    use [`SAMPLE_RATE`]) is beat-tracked ([`beats`]: the same onset
//!    arithmetic as beat markers). Bars are four beats; the bar's first
//!    beat is the phase (0 to 3) whose beats carry the strongest onsets
//!    (the strongest within a quarter beat of each). Each beat is first
//!    moved to the strongest onset within an eighth of a beat, as the
//!    tracker's even grid drifts from the music a little each beat.
//! 2. **What each beat sounds like.** Over each beat a 2048-point Hann FFT
//!    every 1024 samples gives the power of the 12 pitch classes (55 Hz to
//!    4 kHz, nearest semitone) and of 8 log bands (60 Hz to 7 kHz, in dB).
//!    Two beats are alike by `0.6 × correlation of their pitch classes +
//!    0.4 × (1 − mean band difference / 12 dB, at least 0)`.
//! 3. **Joins.** A jump plays up to one bar line and carries on from
//!    another. Its score is the better of: the mean likeness of the two
//!    beats before and the two after each bar line (what is heard on each
//!    side of the join matches what would have been there), and the mean
//!    likeness of the bar before the join and the bar after it, beat by
//!    beat (it follows on as bars within a section do: cutting a whole
//!    middle section out between two alike ones).
//! 4. **The plan.** Shorter: one cut removing at most what is needed and
//!    less than a bar short of it, best score first. Longer: one loop
//!    adding at least what is needed and less than a bar over; when no
//!    loop is that long, the best loop of two bars or more repeats and a
//!    second loop makes up the rest (each may be up to 50 ms over, the
//!    beat grid being only so exact). Whatever is still over comes off the
//!    end, with a 1 s fade-out. Too little music, or no steady beat: a
//!    plain trim (shorter) or an error (longer).
//! 5. **Pieces.** The plan as clips of the same source, back to back on
//!    the timeline in ticks (120 000 a second, as both editors store
//!    them), each but the last crossfading into the next over 0.1 s with
//!    equal power (row 84's crossfade).
//!
//! A join's score drops by 0.05 for each bar it leaves to be cut off the
//! end, so of two nearly alike joins the one keeping the ending wins.
//! Scores are compared rounded to a millionth, ties going to the smaller
//! trim, then the earlier bar, so the two editors pick the same joins.
//! Checked against `fixtures/music-fit/cases.json`.

pub mod beats;

use serde::{Deserialize, Serialize};

/// The rate both editors decode the music at.
pub const SAMPLE_RATE: u32 = 16_000;
pub const TICKS_PER_SECOND: i64 = 120_000;
pub const FFT_SIZE: usize = 2048;
pub const FFT_HOP: usize = 1024;
pub const BEATS_PER_BAR: usize = 4;
pub const PITCH_CLASSES: usize = 12;
pub const BANDS: usize = 8;
/// Each join's crossfade: 0.1 s.
pub const CROSSFADE_TICKS: i64 = 12_000;
/// The fade-out when the end is cut short.
pub const TRIM_FADE_SECONDS: f64 = 1.0;
pub const MAX_PIECES: usize = 64;
pub const MIN_TARGET_SECONDS: f64 = 1.0;
pub const MAX_TARGET_SECONDS: f64 = 1800.0;
/// A length this close to the music's already fits.
pub const MIN_CHANGE_SECONDS: f64 = 0.05;
/// How far a cut or loop may overshoot what is needed: the music then
/// ends this much early, or plays on into the source past the clip.
pub const SLACK_SECONDS: f64 = 0.05;
const EPS: f64 = 1e-6;
const CHROMA_LOW_HZ: f64 = 55.0;
const CHROMA_HIGH_HZ: f64 = 4000.0;
const BAND_LOW_HZ: f64 = 60.0;
const BAND_HIGH_HZ: f64 = 7000.0;

/// Why music could not be fitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FitError {
    BadLength,
    AlreadyFits,
    NoBeat,
    TooShort,
    TooManyPieces,
}

impl FitError {
    pub fn message(self) -> &'static str {
        match self {
            FitError::BadLength => "Pick a length between 1 second and 30 minutes",
            FitError::AlreadyFits => "The music is already that long",
            FitError::NoBeat => "No steady beat was found in the music, so there are no bars to cut or loop",
            FitError::TooShort => "The music is too short to loop: it needs a few bars",
            FitError::TooManyPieces => "That is too long for this music: it would take more than 64 pieces",
        }
    }
}

/// The music's beats and what each sounds like.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub bpm: f64,
    /// Beat times, seconds from the start of the samples.
    pub beats: Vec<f64>,
    /// Which beat (0 to 3) starts the first bar.
    pub downbeat: usize,
    pub chroma: Vec<[f64; PITCH_CLASSES]>,
    /// Band power per beat, dB.
    pub bands: Vec<[f64; BANDS]>,
    pub seconds: f64,
}

impl Analysis {
    pub fn period(&self) -> f64 {
        60.0 / self.bpm
    }

    pub fn bar(&self) -> f64 {
        BEATS_PER_BAR as f64 * self.period()
    }

    /// How alike beats `i` and `j` sound, about -0.6 to 1.
    pub fn likeness(&self, i: usize, j: usize) -> f64 {
        let chroma = correlation(&self.chroma[i], &self.chroma[j]);
        let difference: f64 = (0..BANDS).map(|band| (self.bands[i][band] - self.bands[j][band]).abs()).sum::<f64>() / BANDS as f64;
        0.6 * chroma + 0.4 * (1.0 - difference / 12.0).max(0.0)
    }

    /// The bar lines a join can use: those with a bar before and after.
    pub fn bar_lines(&self) -> Vec<usize> {
        let count = self.beats.len();
        (self.downbeat..count).step_by(BEATS_PER_BAR).filter(|&beat| beat >= BEATS_PER_BAR && beat + BEATS_PER_BAR <= count).collect()
    }

    /// The score of playing up to bar line `from` and carrying on from
    /// `to`: the better of how well each side of the join matches what
    /// would have been there (the two beats either side of the two bar
    /// lines), and how well the bar after the join follows the bar before
    /// it (beat by beat, as bars in one section do).
    pub fn join_score(&self, from: usize, to: usize) -> f64 {
        let context = (-2..=1)
            .map(|offset: isize| self.likeness((from as isize + offset) as usize, (to as isize + offset) as usize))
            .sum::<f64>()
            / 4.0;
        let follow = (0..BEATS_PER_BAR).map(|beat| self.likeness(from - BEATS_PER_BAR + beat, to + beat)).sum::<f64>() / BEATS_PER_BAR as f64;
        context.max(follow)
    }
}

fn correlation(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let mean_a = a.iter().sum::<f64>() / n;
    let mean_b = b.iter().sum::<f64>() / n;
    let (mut num, mut da, mut db) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        num += (x - mean_a) * (y - mean_b);
        da += (x - mean_a) * (x - mean_a);
        db += (y - mean_b) * (y - mean_b);
    }
    if da <= 0.0 || db <= 0.0 { 0.0 } else { num / (da * db).sqrt() }
}

/// In-place radix-2 FFT (`re.len()` a power of two).
fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -2.0 * std::f64::consts::PI / len as f64;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (sin, cos) = (angle * k as f64).sin_cos();
                let a = start + k;
                let b = a + len / 2;
                let tr = re[b] * cos - im[b] * sin;
                let ti = re[b] * sin + im[b] * cos;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
}

/// The band edges, Hz: 8 log-spaced bands from 60 Hz to 7 kHz.
pub fn band_edges() -> [f64; BANDS + 1] {
    std::array::from_fn(|index| BAND_LOW_HZ * (BAND_HIGH_HZ / BAND_LOW_HZ).powf(index as f64 / BANDS as f64))
}

/// Pitch-class and band power for each beat.
fn beat_features(samples: &[f32], rate: u32, beats: &[f64], period: f64, seconds: f64) -> (Vec<[f64; PITCH_CLASSES]>, Vec<[f64; BANDS]>) {
    let rate_f = rate as f64;
    let len = samples.len();
    let window: Vec<f64> =
        (0..FFT_SIZE).map(|index| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * index as f64 / FFT_SIZE as f64).cos()).collect();
    let edges = band_edges();
    // Which pitch class and band each bin feeds.
    let bins: Vec<(Option<usize>, Option<usize>)> = (0..FFT_SIZE / 2)
        .map(|bin| {
            let hz = bin as f64 * rate_f / FFT_SIZE as f64;
            let class = (bin > 0 && (CHROMA_LOW_HZ..=CHROMA_HIGH_HZ).contains(&hz))
                .then(|| ((12.0 * (hz / 440.0).log2()).round() as i64).rem_euclid(12) as usize);
            let band = if bin > 0 { (0..BANDS).find(|&band| hz >= edges[band] && hz < edges[band + 1]) } else { None };
            (class, band)
        })
        .collect();
    let mut chroma = Vec::with_capacity(beats.len());
    let mut bands = Vec::with_capacity(beats.len());
    let mut re = vec![0.0; FFT_SIZE];
    let mut im = vec![0.0; FFT_SIZE];
    for (index, &beat) in beats.iter().enumerate() {
        let end = if index + 1 < beats.len() { beats[index + 1] } else { (beat + period).min(seconds) };
        let a = ((beat * rate_f).round() as usize).min(len);
        let b = ((end * rate_f).round() as usize).min(len);
        let mut starts = Vec::new();
        let mut start = a;
        while start + FFT_SIZE <= b {
            starts.push(start);
            start += FFT_HOP;
        }
        if starts.is_empty() {
            let centre = (a + b) / 2;
            starts.push(centre.saturating_sub(FFT_SIZE / 2).min(len.saturating_sub(FFT_SIZE)));
        }
        let mut classes = [0.0; PITCH_CLASSES];
        let mut power = [0.0; BANDS];
        for &start in &starts {
            for i in 0..FFT_SIZE {
                re[i] = samples.get(start + i).map_or(0.0, |sample| *sample as f64) * window[i];
                im[i] = 0.0;
            }
            fft(&mut re, &mut im);
            for (bin, (class, band)) in bins.iter().enumerate() {
                let p = re[bin] * re[bin] + im[bin] * im[bin];
                if let Some(class) = class {
                    classes[*class] += p;
                }
                if let Some(band) = band {
                    power[*band] += p;
                }
            }
        }
        let frames = starts.len() as f64;
        chroma.push(classes);
        bands.push(power.map(|p| 10.0 * (p / frames + 1e-12).log10()));
    }
    (chroma, bands)
}

/// Beat-tracks mono samples and describes each beat; `None` without a steady beat.
pub fn analyse(samples: &[f32], rate: u32) -> Option<Analysis> {
    let tracked = beats::track_beats(samples, rate, &beats::BeatOptions::default());
    let bpm = tracked.bpm?;
    if tracked.beats.is_empty() {
        return None;
    }
    let onset = beats::onset_strength(samples, rate);
    let beats = refine_beats(&tracked.beats, &onset, 60.0 / bpm);
    // The strongest onset within a quarter of a beat (the grid drifts a little).
    let reach = (60.0 / bpm / 4.0 / 0.005).round() as isize;
    let strength = |time: f64| -> f64 {
        if onset.is_empty() {
            return 0.0;
        }
        let frame = (time / 0.005).round() as isize;
        (frame - reach..=frame + reach).filter(|f| *f >= 0 && (*f as usize) < onset.len()).map(|f| onset[f as usize]).fold(0.0, f64::max)
    };
    let mut downbeat = 0;
    let mut best = f64::NEG_INFINITY;
    for phase in 0..BEATS_PER_BAR.min(beats.len()) {
        let sum: f64 = beats.iter().skip(phase).step_by(BEATS_PER_BAR).map(|beat| strength(*beat)).sum();
        if sum > best {
            best = sum;
            downbeat = phase;
        }
    }
    let period = 60.0 / bpm;
    let (chroma, bands) = beat_features(samples, rate, &beats, period, tracked.seconds);
    Some(Analysis { bpm, beats, downbeat, chroma, bands, seconds: tracked.seconds })
}

/// Moves each tracked beat to the strongest onset within an eighth of a
/// beat (the tracker's even grid drifts from the music by a little each
/// beat), keeping the beats in order; a beat with no onset near it stays.
fn refine_beats(grid: &[f64], onset: &[f64], period: f64) -> Vec<f64> {
    let reach = (period / 8.0 / 0.005).round() as isize;
    let mut out: Vec<f64> = Vec::with_capacity(grid.len());
    for &beat in grid {
        let frame = (beat / 0.005).round() as isize;
        let mut best: Option<(isize, f64)> = None;
        for candidate in frame - reach..=frame + reach {
            if candidate < 0 || candidate as usize >= onset.len() {
                continue;
            }
            let value = onset[candidate as usize];
            if value > 0.0 && best.is_none_or(|(_, top)| value > top) {
                best = Some((candidate, value));
            }
        }
        let moved = best.map_or(beat, |(candidate, _)| (candidate as f64 * 0.005 * 10_000.0).round() / 10_000.0);
        let time = if out.last().is_some_and(|last| moved <= *last) { beat } else { moved };
        if out.last().is_some_and(|last| time <= *last) {
            continue;
        }
        out.push(time);
    }
    out
}

/// One jump: play up to bar line `from`, carry on from bar line `to`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Jump {
    pub from: usize,
    pub to: usize,
    pub from_seconds: f64,
    pub to_seconds: f64,
    pub score: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlanKind {
    /// Shorter by cutting bars out.
    Cut,
    /// Longer by playing bars again.
    Loop,
    /// Shorter by ending early (too little music to cut).
    Trim,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub kind: PlanKind,
    pub jumps: Vec<Jump>,
    /// Source spans played in order, seconds into the music.
    pub segments: Vec<(f64, f64)>,
    /// Cut off the end to land on the length, seconds.
    pub trimmed: f64,
}

fn quantised(score: f64) -> i64 {
    (score * 1e6).round() as i64
}

/// How much a join's score drops per bar of music it leaves to be cut
/// off the end (or ends early by): a slightly less alike join that keeps
/// the ending beats a slightly more alike one that loses it.
pub const TRIM_COST_PER_BAR: f64 = 0.05;

/// The best jump among `pairs` (from, to) whose length `|t_to - t_from|`
/// is accepted: the highest score less `cost` × `waste` in bars, then the
/// smallest `waste`, then the earliest bar lines.
fn best_jump(
    analysis: &Analysis,
    pairs: impl Iterator<Item = (usize, usize)>,
    accept: impl Fn(f64) -> bool,
    waste: impl Fn(f64) -> f64,
    cost: f64,
) -> Option<Jump> {
    let bar = analysis.bar();
    let mut best: Option<(i64, f64, Jump)> = None;
    for (from, to) in pairs {
        let length = (analysis.beats[to] - analysis.beats[from]).abs();
        if !accept(length) {
            continue;
        }
        let score = analysis.join_score(from, to);
        let lost = waste(length);
        let key = quantised(score - cost * lost / bar);
        let better = match &best {
            None => true,
            Some((best_key, best_lost, best_jump)) => {
                key > *best_key
                    || (key == *best_key
                        && (lost < best_lost - 1e-9
                            || ((lost - best_lost).abs() <= 1e-9 && (from, to) < (best_jump.from, best_jump.to))))
            }
        };
        if better {
            let jump = Jump { from, to, from_seconds: analysis.beats[from], to_seconds: analysis.beats[to], score };
            best = Some((key, lost, jump));
        }
    }
    best.map(|(_, _, jump)| jump)
}

/// Plans how `span` seconds of analysed music become `target` seconds.
/// `analysis` may be `None` (no steady beat): only a trim is possible then.
pub fn plan(analysis: Option<&Analysis>, span: f64, target: f64) -> Result<Plan, FitError> {
    if !target.is_finite() || !(MIN_TARGET_SECONDS - EPS..=MAX_TARGET_SECONDS + EPS).contains(&target) || !(span > 0.0) {
        return Err(FitError::BadLength);
    }
    if (target - span).abs() < MIN_CHANGE_SECONDS {
        return Err(FitError::AlreadyFits);
    }
    let trim = || Plan { kind: PlanKind::Trim, jumps: Vec::new(), segments: vec![(0.0, target)], trimmed: span - target };
    let Some(analysis) = analysis else {
        return if target < span { Ok(trim()) } else { Err(FitError::NoBeat) };
    };
    let bar = analysis.bar();
    let period = analysis.period();
    let lines = analysis.bar_lines();
    let beats = &analysis.beats;
    let mut jumps = Vec::new();
    let kind;
    if target < span {
        let need = span - target;
        let pairs = lines.iter().flat_map(|&from| lines.iter().filter(move |&&to| to > from).map(move |&to| (from, to)));
        // Keep at least a beat after the join once the end is trimmed.
        let pairs = pairs.filter(|&(from, to)| {
            let length = beats[to] - beats[from];
            (span - beats[to]) - (need - length) >= period - EPS
        });
        match best_jump(
            analysis,
            pairs,
            |length| length <= need + SLACK_SECONDS && length > need - bar,
            |length| (need - length).abs(),
            TRIM_COST_PER_BAR,
        ) {
            Some(jump) => {
                jumps.push(jump);
                kind = PlanKind::Cut;
            }
            None => return Ok(trim()),
        }
    } else {
        let add = target - span;
        let loops = || lines.iter().flat_map(|&from| lines.iter().filter(move |&&to| to < from).map(move |&to| (from, to)));
        if loops().next().is_none() {
            return Err(FitError::TooShort);
        }
        let fits = |wanted: f64| move |length: f64| length >= wanted - SLACK_SECONDS && length < wanted + bar;
        if let Some(jump) = best_jump(analysis, loops(), fits(add), |length| (length - add).abs(), TRIM_COST_PER_BAR) {
            jumps.push(jump);
        } else {
            let long = best_jump(analysis, loops(), |length| length >= 2.0 * bar - EPS, |length| -length, 0.0);
            let first = long.or_else(|| best_jump(analysis, loops(), |_| true, |length| -length, 0.0)).ok_or(FitError::TooShort)?;
            let length = first.from_seconds - first.to_seconds;
            let repeats = ((add + SLACK_SECONDS) / length).floor() as usize;
            if repeats + 2 > MAX_PIECES {
                return Err(FitError::TooManyPieces);
            }
            let rest = add - repeats as f64 * length;
            jumps.extend(std::iter::repeat_n(first, repeats));
            if rest > SLACK_SECONDS {
                match best_jump(analysis, loops(), fits(rest), |length| (length - rest).abs(), TRIM_COST_PER_BAR) {
                    Some(second) => jumps.push(second),
                    None => jumps.push(first),
                }
            }
            jumps.sort_by_key(|jump| jump.from);
        }
        kind = PlanKind::Loop;
    }
    if jumps.len() + 1 > MAX_PIECES {
        return Err(FitError::TooManyPieces);
    }
    let mut segments = Vec::with_capacity(jumps.len() + 1);
    let mut at = 0.0;
    for jump in &jumps {
        segments.push((at, jump.from_seconds));
        at = jump.to_seconds;
    }
    segments.push((at, span));
    let total: f64 = segments.iter().map(|(from, to)| to - from).sum();
    let over = total - target;
    let mut trimmed = 0.0;
    if over > EPS {
        let last = segments.last_mut().expect("a segment");
        last.1 -= over;
        trimmed = over;
    }
    Ok(Plan { kind, jumps, segments, trimmed })
}

/// The music clip as both editors store it (ticks; fades in seconds).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Clip {
    pub start: i64,
    pub duration: i64,
    pub trim_start: i64,
    pub trim_end: i64,
    pub fade_in: f64,
    pub fade_out: f64,
}

/// One piece of the fitted music: a clip of the same source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Piece {
    pub start: i64,
    pub duration: i64,
    pub trim_start: i64,
    pub trim_end: i64,
    pub fade_in: f64,
    pub fade_out: f64,
    /// Its crossfade into the next piece, ticks (equal power); 0 on the last.
    pub crossfade: i64,
}

fn ticks(seconds: f64) -> i64 {
    (seconds * TICKS_PER_SECOND as f64).round() as i64
}

/// The plan as clips back to back from the clip's start, `target` ticks in all.
pub fn layout(plan: &Plan, clip: &Clip, target: i64) -> Vec<Piece> {
    let span_end = clip.trim_start + clip.duration;
    let total = span_end + clip.trim_end;
    let count = plan.segments.len();
    let mut pieces = Vec::with_capacity(count);
    let mut used = 0;
    for (index, (from, to)) in plan.segments.iter().enumerate() {
        let last = index + 1 == count;
        let source = clip.trim_start + ticks(*from);
        let duration = if last { target - used } else { ticks(*to) - ticks(*from) };
        // A loop or cut a little over what was needed plays on into the
        // source past the clip, when there is any.
        let duration = duration.min(total - source).max(1);
        let seconds = duration as f64 / TICKS_PER_SECOND as f64;
        let fade_in = if index == 0 { clip.fade_in.min(seconds) } else { 0.0 };
        let fade_out = if last {
            let trim_fade = if plan.trimmed > 0.01 { TRIM_FADE_SECONDS.min(seconds / 2.0) } else { 0.0 };
            clip.fade_out.max(trim_fade).min(seconds)
        } else {
            0.0
        };
        pieces.push(Piece {
            start: clip.start + used,
            duration,
            trim_start: source,
            trim_end: total - source - duration,
            fade_in,
            fade_out,
            crossfade: if last { 0 } else { CROSSFADE_TICKS },
        });
        used += duration;
    }
    pieces
}

/// The fitted music and what was done.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fit {
    pub kind: PlanKind,
    pub pieces: Vec<Piece>,
    pub jumps: Vec<Jump>,
    /// `None` when no steady beat was found (a trim).
    pub bpm: Option<f64>,
    pub trimmed: f64,
}

/// Fits the clip (whose playing span `samples` holds, mono at `rate`) to `target` ticks.
pub fn fit(samples: &[f32], rate: u32, clip: &Clip, target: i64) -> Result<Fit, FitError> {
    let span = clip.duration as f64 / TICKS_PER_SECOND as f64;
    let target_seconds = target as f64 / TICKS_PER_SECOND as f64;
    if !(MIN_TARGET_SECONDS - EPS..=MAX_TARGET_SECONDS + EPS).contains(&target_seconds) {
        return Err(FitError::BadLength);
    }
    if (target_seconds - span).abs() < MIN_CHANGE_SECONDS {
        return Err(FitError::AlreadyFits);
    }
    let analysis = analyse(samples, rate);
    let plan = plan(analysis.as_ref(), span, target_seconds)?;
    let pieces = layout(&plan, clip, target);
    Ok(Fit { kind: plan.kind, pieces, jumps: plan.jumps, bpm: analysis.map(|a| a.bpm), trimmed: plan.trimmed })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Case {
        name: String,
        music: String,
        sample_rate: u32,
        clip: Clip,
        target: i64,
        bpm: Option<f64>,
        downbeat: Option<usize>,
        beats: Vec<f64>,
        kind: Option<PlanKind>,
        jumps: Vec<(usize, usize)>,
        pieces: Vec<Piece>,
        error: Option<FitError>,
    }

    #[derive(Deserialize)]
    struct Fixture {
        /// 16-bit little-endian PCM, base64.
        musics: std::collections::HashMap<String, String>,
        cases: Vec<Case>,
    }

    fn pcm(text: &str) -> Vec<f32> {
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut bytes = Vec::new();
        let (mut acc, mut bits) = (0u32, 0);
        for c in text.bytes().filter(|c| *c != b'=') {
            acc = (acc << 6) | alphabet.iter().position(|a| *a == c).unwrap() as u32;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                bytes.push((acc >> bits) as u8);
            }
        }
        bytes.chunks_exact(2).map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32768.0).collect()
    }

    fn fixture() -> Fixture {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/music-fit/cases.json");
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn matches_the_reference() {
        let fixture = fixture();
        assert!(fixture.cases.len() >= 5);
        let musics: std::collections::HashMap<&str, Vec<f32>> =
            fixture.musics.iter().map(|(name, text)| (name.as_str(), pcm(text))).collect();
        for case in &fixture.cases {
            let samples = &musics[case.music.as_str()];
            let analysis = analyse(samples, case.sample_rate);
            assert_eq!(analysis.as_ref().map(|a| (a.bpm * 1000.0).round()), case.bpm.map(|b| (b * 1000.0).round()), "{}", case.name);
            if let Some(analysis) = &analysis {
                assert_eq!(Some(analysis.downbeat), case.downbeat, "{}", case.name);
                assert_eq!(analysis.beats, case.beats, "{}", case.name);
            }
            match fit(samples, case.sample_rate, &case.clip, case.target) {
                Ok(fit) => {
                    assert_eq!(case.error, None, "{}", case.name);
                    assert_eq!(Some(fit.kind), case.kind, "{}", case.name);
                    let jumps: Vec<(usize, usize)> = fit.jumps.iter().map(|jump| (jump.from, jump.to)).collect();
                    assert_eq!(jumps, case.jumps, "{}", case.name);
                    assert_eq!(fit.pieces.len(), case.pieces.len(), "{}", case.name);
                    for (got, want) in fit.pieces.iter().zip(&case.pieces) {
                        assert_eq!((got.start, got.duration, got.trim_start, got.trim_end, got.crossfade),
                            (want.start, want.duration, want.trim_start, want.trim_end, want.crossfade), "{}", case.name);
                        assert!((got.fade_in - want.fade_in).abs() < 1e-9 && (got.fade_out - want.fade_out).abs() < 1e-9, "{}", case.name);
                    }
                    let total: i64 = fit.pieces.iter().map(|piece| piece.duration).sum();
                    assert!(total <= case.target && total >= case.target - 6_000, "{}", case.name);
                }
                Err(error) => assert_eq!(Some(error), case.error, "{}", case.name),
            }
        }
    }

    #[test]
    fn plans_cuts_and_loops_on_a_synthetic_analysis() {
        // 120 BPM, 32 bars, sections A A B B A A B B ... (pitch classes differ).
        let beats: Vec<f64> = (0..128).map(|beat| beat as f64 * 0.5).collect();
        let chroma: Vec<[f64; PITCH_CLASSES]> = beats
            .iter()
            .enumerate()
            .map(|(beat, _)| {
                let mut c = [0.1; PITCH_CLASSES];
                let section = (beat / 8) % 2;
                c[(beat % 4) + section * 6] = 1.0;
                c
            })
            .collect();
        let analysis = Analysis { bpm: 120.0, beats, downbeat: 0, chroma, bands: vec![[-20.0; BANDS]; 128], seconds: 64.0 };
        let cut = plan(Some(&analysis), 64.0, 48.0).unwrap();
        assert_eq!(cut.kind, PlanKind::Cut);
        assert_eq!(cut.jumps.len(), 1);
        let jump = cut.jumps[0];
        assert_eq!(jump.to_seconds - jump.from_seconds, 16.0);
        assert!(jump.score > 0.99, "{}", jump.score);
        assert_eq!(cut.trimmed, 0.0);
        let total: f64 = cut.segments.iter().map(|(a, b)| b - a).sum();
        assert!((total - 48.0).abs() < 1e-9);

        let long = plan(Some(&analysis), 64.0, 64.0 + 90.0).unwrap();
        assert_eq!(long.kind, PlanKind::Loop);
        let total: f64 = long.segments.iter().map(|(a, b)| b - a).sum();
        assert!((total - 154.0).abs() < 1e-9, "{total}");
        assert!(long.jumps.len() >= 2);
        for pair in long.jumps.windows(2) {
            assert!(pair[1].from > pair[0].to, "reachable");
        }
        assert_eq!(plan(Some(&analysis), 64.0, 64.02), Err(FitError::AlreadyFits));
        assert_eq!(plan(None, 64.0, 70.0), Err(FitError::NoBeat));
        assert_eq!(plan(None, 64.0, 30.0).unwrap().kind, PlanKind::Trim);
        assert_eq!(plan(Some(&analysis), 64.0, 0.5), Err(FitError::BadLength));
        assert_eq!(plan(Some(&analysis), 64.0, 64.0 * 70.0), Err(FitError::BadLength));
    }

    #[test]
    fn lays_pieces_back_to_back() {
        let plan = Plan {
            kind: PlanKind::Loop,
            jumps: Vec::new(),
            segments: vec![(0.0, 8.0), (4.0, 8.0), (4.0, 9.5)],
            trimmed: 0.5,
        };
        let clip = Clip { start: 120_000, duration: 10 * 120_000, trim_start: 240_000, trim_end: 60_000, fade_in: 0.5, fade_out: 0.2 };
        let pieces = layout(&plan, &clip, 2_100_000);
        assert_eq!(pieces.len(), 3);
        assert_eq!(pieces[0].start, 120_000);
        assert_eq!(pieces[1].start, pieces[0].start + pieces[0].duration);
        assert_eq!(pieces[1].trim_start, 240_000 + 4 * 120_000);
        assert_eq!((pieces[0].duration, pieces[1].duration, pieces[2].duration), (960_000, 480_000, 660_000));
        let total = 240_000 + 1_200_000 + 60_000;
        for piece in &pieces {
            assert_eq!(piece.trim_start + piece.duration + piece.trim_end, total);
        }
        assert_eq!((pieces[0].crossfade, pieces[1].crossfade, pieces[2].crossfade), (CROSSFADE_TICKS, CROSSFADE_TICKS, 0));
        assert_eq!((pieces[0].fade_in, pieces[1].fade_in, pieces[2].fade_out), (0.5, 0.0, 1.0));
    }
}
