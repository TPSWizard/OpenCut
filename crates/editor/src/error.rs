use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum EditorError {
    #[error("track {0} does not exist")]
    TrackNotFound(String),
    #[error("element {0} does not exist")]
    ElementNotFound(String),
    #[error("media {0} does not exist")]
    MediaNotFound(String),
    #[error("{0}")]
    InvalidParams(String),
    #[error("the main video track cannot be removed")]
    CannotRemoveMainTrack,
    #[error("a {element} element cannot go on a {track} track")]
    TrackKindMismatch { element: String, track: String },
    #[error("the element would overlap another element on that track")]
    Overlap,
    #[error("nothing to {0}")]
    NothingTo(&'static str),
}
