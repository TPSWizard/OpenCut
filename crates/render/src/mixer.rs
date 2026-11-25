//! Mixes the project's audio clips into interleaved float samples for a
//! playback device. One decoder process per clip that is currently audible.
//! Each track's part of the mix also goes through its own loudness meter
//! (the mixer panel's track meters).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use editor::AudioClipPlan;
use media::{AudioReader, AudioSettings, Ffmpeg, MediaError};

struct ActiveClip {
    clip_index: usize,
    reader: AudioReader,
}

pub struct Mixer {
    ffmpeg: Arc<Ffmpeg>,
    settings: AudioSettings,
    /// Where retimed clips are rendered to before they are read.
    cache_dir: std::path::PathBuf,
    clips: Vec<AudioClipPlan>,
    active: Vec<ActiveClip>,
    /// Timeline position of the next sample frame to produce.
    position_frames: i64,
    scratch: Vec<f32>,
    /// A loudness meter per track id, fed with that track's part of the mix.
    track_meters: Arc<Mutex<HashMap<String, editor::LoudnessMeter>>>,
}

/// The track meters a mixer keeps, readable from another thread.
pub type TrackMeters = Arc<Mutex<HashMap<String, editor::LoudnessMeter>>>;

/// Decoder drift this large (in seconds) means the reader is reopened.
const RESYNC_TOLERANCE_SECONDS: f64 = 0.05;

/// A keyframed volume is evaluated once per this many frames (0.7 ms at
/// 48 kHz), which is well under what the ear resolves.
const GAIN_BLOCK_FRAMES: usize = 32;

impl Mixer {
    pub fn new(ffmpeg: Arc<Ffmpeg>, settings: AudioSettings) -> Self {
        Self {
            ffmpeg,
            settings,
            cache_dir: crate::retime_audio::default_cache_dir(),
            clips: Vec::new(),
            active: Vec::new(),
            position_frames: 0,
            scratch: Vec::new(),
            track_meters: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// The per-track meters, to read while playing (they run as far ahead
    /// of the sound as the mixer runs ahead of the device).
    pub fn track_meters(&self) -> TrackMeters {
        Arc::clone(&self.track_meters)
    }

    fn reset_meters(&self) {
        self.track_meters.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    pub fn settings(&self) -> AudioSettings {
        self.settings
    }

    /// Replaces the clips (after an edit). Readers for clips that did not
    /// change keep running.
    pub fn set_clips(&mut self, clips: Vec<AudioClipPlan>) {
        // Clips with a speed change or reverse play from a rendered copy.
        let clips: Vec<AudioClipPlan> = clips
            .into_iter()
            .map(|clip| {
                if crate::retime_audio::needs_render(&clip) {
                    crate::retime_audio::rendered_clip(&self.ffmpeg, &clip, &self.cache_dir).unwrap_or_else(|error| {
                        eprintln!("mixer: retimed audio for {}: {error}", clip.element_id);
                        clip
                    })
                } else {
                    clip
                }
            })
            .collect();
        let mut kept = Vec::new();
        for active in self.active.drain(..) {
            let old = &self.clips[active.clip_index];
            if let Some(new_index) = clips.iter().position(|clip| {
                clip.element_id == old.element_id
                    && clip.path == old.path
                    && clip.timeline_start == old.timeline_start
                    && clip.source_start == old.source_start
            }) {
                kept.push(ActiveClip {
                    clip_index: new_index,
                    reader: active.reader,
                });
            }
        }
        self.clips = clips;
        self.active = kept;
    }

    pub fn position_seconds(&self) -> f64 {
        self.position_frames as f64 / self.settings.sample_rate as f64
    }

    pub fn seek(&mut self, seconds: f64) {
        self.position_frames = (seconds.max(0.0) * self.settings.sample_rate as f64).round() as i64;
        self.active.clear();
        self.reset_meters();
    }

    /// Stops every decoder (on pause). Playback resumes from the same spot.
    pub fn stop(&mut self) {
        self.active.clear();
        self.reset_meters();
    }

    /// Fills `out` (interleaved, `settings.channels` wide) with the mix from
    /// the current position onward and advances the position.
    pub fn fill(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        let channels = self.settings.channels as usize;
        let frames = out.len() / channels;
        if frames == 0 {
            return;
        }
        let sample_rate = self.settings.sample_rate as f64;
        let buffer_start = self.position_frames;
        let buffer_end = buffer_start + frames as i64;
        // Each track's part of this chunk, for its meter.
        let mut track_parts: HashMap<String, Vec<f32>> = HashMap::new();

        for clip_index in 0..self.clips.len() {
            let clip = &self.clips[clip_index];
            let clip_start = (clip.timeline_start.to_seconds_f64() * sample_rate).round() as i64;
            let clip_end = (clip.timeline_end().to_seconds_f64() * sample_rate).round() as i64;
            let overlap_start = buffer_start.max(clip_start);
            let overlap_end = buffer_end.min(clip_end);
            if overlap_end <= overlap_start {
                continue;
            }
            let source_time =
                clip.source_start.to_seconds_f64() + (overlap_start - clip_start) as f64 / sample_rate;
            let count = (overlap_end - overlap_start) as usize;
            let mut scratch = std::mem::take(&mut self.scratch);
            scratch.resize(count * channels, 0.0);
            let read = match self.reader_for(clip_index, source_time) {
                Some(reader) => match reader.read(&mut scratch) {
                    Ok(read) => read,
                    Err(error) => {
                        eprintln!("mixer: {error}");
                        0
                    }
                },
                None => 0,
            };
            let clip = &self.clips[clip_index];
            if (clip.pan != 0.0 || clip.track_pan != 0.0) && channels == 2 {
                // The clip's pan, then its track's.
                for frame in scratch[..read].chunks_exact_mut(2) {
                    let (left, right) = editor::pan_clip_and_track(clip.pan, clip.track_pan, frame[0] as f64, frame[1] as f64);
                    frame[0] = left as f32;
                    frame[1] = right as f32;
                }
            }
            let offset = (overlap_start - buffer_start) as usize * channels;
            let part = track_parts.entry(clip.track_id.clone()).or_insert_with(|| vec![0.0; out.len()]);
            if clip.is_animated() {
                // Keyframed volume: one gain per short block of frames.
                let block_samples = GAIN_BLOCK_FRAMES * channels;
                for (block_index, block) in scratch[..read].chunks(block_samples).enumerate() {
                    let frame = overlap_start + (block_index * GAIN_BLOCK_FRAMES) as i64;
                    let gain = clip.gain_at_seconds(frame as f64 / sample_rate) as f32;
                    let start = offset + block_index * block_samples;
                    for (index, sample) in block.iter().enumerate() {
                        out[start + index] += sample * gain;
                        part[start + index] += sample * gain;
                    }
                }
            } else {
                let gain = clip.gain() as f32;
                for (index, sample) in scratch[..read].iter().enumerate() {
                    out[offset + index] += sample * gain;
                    part[offset + index] += sample * gain;
                }
            }
            self.scratch = scratch;
        }
        for sample in out.iter_mut() {
            *sample = sample.clamp(-1.0, 1.0);
        }
        {
            let mut meters = self.track_meters.lock().unwrap_or_else(|e| e.into_inner());
            let silence = vec![0.0f32; out.len()];
            // Tracks heard before but silent now decay with silence.
            let quiet: Vec<String> = meters.keys().filter(|id| !track_parts.contains_key(*id)).cloned().collect();
            for id in quiet {
                if let Some(meter) = meters.get_mut(&id) {
                    meter.push_interleaved(&silence);
                }
            }
            for (id, part) in &track_parts {
                meters
                    .entry(id.clone())
                    .or_insert_with(|| editor::LoudnessMeter::new(self.settings.sample_rate, channels))
                    .push_interleaved(part);
            }
        }
        self.position_frames = buffer_end;
        // Drop readers of clips that have ended.
        let clips = &self.clips;
        self.active.retain(|active| {
            let clip = &clips[active.clip_index];
            (clip.timeline_end().to_seconds_f64() * sample_rate).round() as i64 > buffer_end
        });
    }

    fn reader_for(&mut self, clip_index: usize, source_time: f64) -> Option<&mut AudioReader> {
        let position = self
            .active
            .iter()
            .position(|active| active.clip_index == clip_index);
        if let Some(position) = position {
            let drift = (self.active[position].reader.next_time_seconds() - source_time).abs();
            if drift <= RESYNC_TOLERANCE_SECONDS {
                return Some(&mut self.active[position].reader);
            }
            self.active.remove(position);
        }
        let clip = &self.clips[clip_index];
        match AudioReader::open(&self.ffmpeg, &clip.path, source_time, self.settings) {
            Ok(reader) => {
                self.active.push(ActiveClip { clip_index, reader });
                self.active.last_mut().map(|active| &mut active.reader)
            }
            Err(MediaError::Io(error)) => {
                eprintln!("mixer: cannot start ffmpeg: {error}");
                None
            }
            Err(error) => {
                eprintln!("mixer: {error}");
                None
            }
        }
    }
}
