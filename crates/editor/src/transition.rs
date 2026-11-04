//! Where a clip's transition plays on the timeline.
//!
//! A transition belongs to the clip it leads out of and is centred on the
//! cut to the next clip on the same track: half of it plays over the end of
//! the outgoing clip, half over the start of the incoming one, so the
//! timeline's durations never change. Each side keeps playing through its
//! half from the source material beyond its trim ("handles"); a source with
//! no handle holds its edge frame. A clip with nothing adjacent after it
//! transitions out to nothing over its own last moments.

use time::MediaTime;

use crate::document::{Element, Track};

/// How far apart two clips may be and still count as one cut: 10 ms, under
/// a frame at any editing frame rate, so snapped placement always qualifies.
pub const ADJACENCY_TOLERANCE: MediaTime = MediaTime::from_ticks(1200);

/// The span of a transition and the clip it leads into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransitionWindow {
    pub start: MediaTime,
    /// Exclusive.
    pub end: MediaTime,
    /// The incoming clip, or `None` for a transition out to nothing.
    pub to: Option<String>,
}

impl TransitionWindow {
    pub fn contains(&self, time: MediaTime) -> bool {
        time >= self.start && time < self.end
    }

    pub fn duration(&self) -> MediaTime {
        self.end - self.start
    }

    /// 0 at the start of the window, 1 at its end.
    pub fn progress(&self, time: MediaTime) -> f32 {
        let length = self.duration().as_ticks();
        if length <= 0 {
            return 1.0;
        }
        ((time - self.start).as_ticks() as f64 / length as f64).clamp(0.0, 1.0) as f32
    }
}

/// Whether `next` starts where `element` ends.
pub fn is_adjacent(element: &Element, next: &Element) -> bool {
    (next.start_time - element.end_time()).as_ticks().abs() <= ADJACENCY_TOLERANCE.as_ticks()
}

/// The first visible element on the track that follows `element` directly.
pub fn next_adjacent<'a>(track: &'a Track, element: &Element) -> Option<&'a Element> {
    track
        .elements
        .iter()
        .filter(|candidate| candidate.id != element.id && !candidate.hidden && candidate.kind.is_visual())
        .filter(|candidate| is_adjacent(element, candidate))
        .min_by_key(|candidate| candidate.start_time)
}

/// The window of `element`'s transition, given the clip after it; a `next`
/// that is not adjacent counts as none. `None` when the element has no
/// transition or it would be empty.
pub fn transition_window(element: &Element, next: Option<&Element>) -> Option<TransitionWindow> {
    let transition = element.transition.as_ref()?;
    if transition.duration <= MediaTime::ZERO || element.duration <= MediaTime::ZERO {
        return None;
    }
    let cut = element.end_time();
    let window = match next {
        Some(next) if next.duration > MediaTime::ZERO && is_adjacent(element, next) => {
            // Never longer than either clip, so the window stays inside both.
            let length = transition.duration.min(element.duration).min(next.duration);
            let half = MediaTime::from_ticks(length.as_ticks() / 2);
            TransitionWindow {
                start: cut - half,
                end: cut - half + length,
                to: Some(next.id.clone()),
            }
        }
        _ => {
            let length = transition.duration.min(element.duration);
            TransitionWindow {
                start: cut - length,
                end: cut,
                to: None,
            }
        }
    };
    (window.end > window.start).then_some(window)
}

/// The window of the transition on `element`'s track that `element` leads
/// out of, if it has one.
pub fn window_of<'a>(track: &'a Track, element: &'a Element) -> Option<TransitionWindow> {
    transition_window(element, next_adjacent(track, element))
}

/// The window of the transition that leads into `element`, from the clip
/// before it, if there is one.
pub fn window_into(track: &Track, element: &Element) -> Option<TransitionWindow> {
    track
        .elements
        .iter()
        .filter(|candidate| candidate.id != element.id && !candidate.hidden && candidate.transition.is_some())
        .filter(|candidate| is_adjacent(candidate, element))
        .filter_map(|candidate| {
            let window = transition_window(candidate, next_adjacent(track, candidate))?;
            (window.to.as_deref() == Some(element.id.as_str())).then_some(window)
        })
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{ElementKind, Lane, Transition, TrackKind};
    use crate::params::Params;

    fn seconds(value: f64) -> MediaTime {
        MediaTime::from_seconds_f64(value).unwrap()
    }

    fn clip(id: &str, start: f64, duration: f64) -> Element {
        Element {
            id: id.to_owned(),
            name: id.to_owned(),
            kind: ElementKind::Image {
                media_id: "m".to_owned(),
            },
            start_time: seconds(start),
            duration: seconds(duration),
            trim_start: MediaTime::ZERO,
            trim_end: MediaTime::ZERO,
            source_duration: None,
            hidden: false,
            params: Params::new(),
            animations: Default::default(),
            effects: Vec::new(),
            masks: Vec::new(),
            transition: None,
            words: Vec::new(),
        }
    }

    fn with_transition(mut element: Element, duration: f64) -> Element {
        element.transition = Some(Transition {
            kind: "Fade".to_owned(),
            duration: seconds(duration),
            params: Params::new(),
        });
        element
    }

    #[test]
    fn a_transition_is_centred_on_the_cut() {
        let a = with_transition(clip("a", 0.0, 4.0), 1.0);
        let b = clip("b", 4.0, 4.0);
        let window = transition_window(&a, Some(&b)).unwrap();
        assert_eq!(window.start, seconds(3.5));
        assert_eq!(window.end, seconds(4.5));
        assert_eq!(window.to.as_deref(), Some("b"));
        assert!(window.contains(seconds(3.5)));
        assert!(!window.contains(seconds(4.5)));
        assert_eq!(window.progress(seconds(3.5)), 0.0);
        assert_eq!(window.progress(seconds(4.0)), 0.5);
        assert_eq!(window.progress(seconds(4.5)), 1.0);
    }

    #[test]
    fn a_transition_never_outgrows_either_clip() {
        let a = with_transition(clip("a", 0.0, 4.0), 3.0);
        let b = clip("b", 4.0, 1.0);
        let window = transition_window(&a, Some(&b)).unwrap();
        assert_eq!(window.duration(), seconds(1.0));
        assert_eq!(window.start, seconds(3.5));
    }

    #[test]
    fn without_a_neighbour_it_fades_out_over_the_end() {
        let a = with_transition(clip("a", 0.0, 4.0), 1.0);
        let window = transition_window(&a, None).unwrap();
        assert_eq!(window.start, seconds(3.0));
        assert_eq!(window.end, seconds(4.0));
        assert_eq!(window.to, None);
        let far = clip("c", 6.0, 2.0);
        assert!(!is_adjacent(&a, &far));
        assert_eq!(transition_window(&a, Some(&far)).unwrap().to, None, "a gap is not a cut");
    }

    #[test]
    fn track_helpers_find_the_neighbours() {
        let mut track = Track::new(TrackKind::Video, Lane::Main, "Main");
        track.elements.push(clip("b", 4.0, 2.0));
        track.elements.push(with_transition(clip("a", 0.0, 4.0), 1.0));
        track.elements.push(clip("c", 6.0, 2.0));
        let a = track.elements[1].clone();
        let b = track.elements[0].clone();
        assert_eq!(next_adjacent(&track, &a).map(|e| e.id.as_str()), Some("b"));
        assert_eq!(window_of(&track, &a).unwrap().to.as_deref(), Some("b"));
        assert_eq!(window_into(&track, &b).unwrap().start, seconds(3.5));
        assert!(window_into(&track, &a).is_none());
        let c = track.elements[2].clone();
        assert!(window_into(&track, &c).is_none(), "b has no transition");
    }

    #[test]
    fn no_transition_or_zero_length_gives_no_window() {
        let a = clip("a", 0.0, 4.0);
        assert!(transition_window(&a, None).is_none());
        let a = with_transition(clip("a", 0.0, 4.0), 0.0);
        assert!(transition_window(&a, None).is_none());
    }
}
