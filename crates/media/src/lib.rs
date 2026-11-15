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
mod codecs;
mod denoise;
mod encode;
mod error;
mod ffmpeg;
mod image_decode;
mod motion;
mod scenes;
mod probe;
mod proxy;
mod upscale;
mod video;

pub use audio::{AudioReader, AudioSettings};
pub use denoise::denoise_to_wav;
pub use codecs::{
    MAX_BITRATE_MBPS, MIN_BITRATE_MBPS, VideoCodec, available_codecs, codecs_for, default_codec, is_hardware_encoder,
    listed_encoders, parse_bitrate_mbps, pick_encoder, resolve_codec,
};
pub use encode::{
    AudioMixClip, EncodeFormat, EncodeQuality, EncodeSettings, Encoder, GainEase, GainKey, PlatformEncode, settings_codec,
    video_encoder,
};
pub use error::MediaError;
pub use ffmpeg::Ffmpeg;
pub use image_decode::{decode_image, is_image_path};
pub use motion::{
    StabiliseStrength, can_interpolate, can_stabilise, interpolate_filter, interpolate_video, listed_filters, stabilise_filters,
    stabilise_video,
};
pub use probe::{MediaInfo, MediaKind, probe};
pub use proxy::{AUTO_PROXY_PIXELS, PROXY_LONG_EDGE, existing_proxy, make_proxy, proxy_path, proxy_size, wants_proxy};
pub use scenes::{SceneScore, parse_scdet, scene_scores};
pub use upscale::{MAX_UPSCALED_SIDE, UpscaleModel, locate_realesrgan, upscale_image, upscale_video, upscaled_size};
pub use video::{Frame, VideoReader, VideoSource, thumbnail};
