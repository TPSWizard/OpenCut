//! Native media I/O for OpenCut. Video and audio go through the FFmpeg
//! programs (`ffmpeg`, `ffprobe`) over pipes: no FFmpeg headers, no
//! bindings, no unsafe, and the same formats everywhere. Images decode
//! in-process with the `image` crate.
//!
//! The programs are found by [`Ffmpeg::locate`]: `OPENCUT_FFMPEG_DIR`, the
//! pinned build that `moon run media:setup` puts in `.cache/media/`, the
//! folder of the running executable, then `PATH`.
//!
//! Decoded frames are BGRA, 8 bits per channel, tightly packed: what the
//! GPU compositor's native texture format and GPUI's images both want.

mod audio;
mod denoise;
mod encode;
mod error;
mod ffmpeg;
mod image_decode;
mod scenes;
mod probe;
mod proxy;
mod video;

pub use audio::{AudioReader, AudioSettings};
pub use denoise::denoise_to_wav;
pub use encode::{AudioMixClip, EncodeFormat, EncodeQuality, EncodeSettings, Encoder, GainEase, GainKey, PlatformEncode};
pub use error::MediaError;
pub use ffmpeg::Ffmpeg;
pub use image_decode::{decode_image, is_image_path};
pub use probe::{MediaInfo, MediaKind, probe};
pub use proxy::{AUTO_PROXY_PIXELS, PROXY_LONG_EDGE, existing_proxy, make_proxy, proxy_path, proxy_size, wants_proxy};
pub use scenes::{SceneScore, parse_scdet, scene_scores};
pub use video::{Frame, VideoReader, VideoSource, thumbnail};
