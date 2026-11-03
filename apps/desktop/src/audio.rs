//! Audio for playback: a mixer thread keeps a ring buffer ahead of the
//! device callback, which only copies samples out. The callback also
//! counts the frames it has played, which is the clock the picture
//! follows while audio is running (see [`AudioOutput::position_seconds`]).

use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use editor::AudioClipPlan;
use media::{AudioSettings, Ffmpeg};
use render::Mixer;

enum AudioCommand {
    SetClips(Vec<AudioClipPlan>),
    Seek(f64),
    Play,
    Pause,
}

struct Ring {
    samples: VecDeque<f32>,
    playing: bool,
    /// Timeline position the device started playing from at the last seek.
    base_seconds: f64,
    /// Frames the device has taken since then (silence on underrun
    /// counts too: time still passes).
    played_frames: u64,
}

impl Ring {
    fn new(sample_rate: u32) -> Self {
        Self {
            samples: VecDeque::with_capacity(sample_rate as usize),
            playing: false,
            base_seconds: 0.0,
            played_frames: 0,
        }
    }

    /// Fills a device buffer and advances the clock.
    fn consume(&mut self, data: &mut [f32], channels: usize) {
        if !self.playing {
            data.fill(0.0);
            return;
        }
        for sample in data.iter_mut() {
            *sample = self.samples.pop_front().unwrap_or(0.0);
        }
        self.played_frames += (data.len() / channels.max(1)) as u64;
    }

    fn seek(&mut self, seconds: f64) {
        self.samples.clear();
        self.base_seconds = seconds;
        self.played_frames = 0;
    }

    fn position_seconds(&self, sample_rate: u32) -> Option<f64> {
        self.playing
            .then(|| self.base_seconds + self.played_frames as f64 / sample_rate.max(1) as f64)
    }
}

pub(crate) struct AudioOutput {
    _stream: cpal::Stream,
    commands: Sender<AudioCommand>,
    ring: Arc<Mutex<Ring>>,
    sample_rate: u32,
}

impl AudioOutput {
    /// Opens the default output device. `None` (with a reason) when there
    /// is no device or no float output format.
    pub(crate) fn open(ffmpeg: Arc<Ffmpeg>) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "no audio output device".to_owned())?;
        let config = device
            .default_output_config()
            .map_err(|error| error.to_string())?;
        if config.sample_format() != cpal::SampleFormat::F32 {
            return Err(format!(
                "the output device wants {} samples; only f32 is supported",
                config.sample_format()
            ));
        }
        let channels = config.channels().clamp(1, 2);
        let sample_rate = config.sample_rate().0;
        let stream_config = cpal::StreamConfig {
            channels,
            sample_rate: config.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };
        let settings = AudioSettings {
            sample_rate,
            channels,
        };

        let ring = Arc::new(Mutex::new(Ring::new(sample_rate)));
        let callback_ring = Arc::clone(&ring);
        let callback_channels = channels as usize;
        let stream = device
            .build_output_stream(
                &stream_config,
                move |data: &mut [f32], _| {
                    callback_ring
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .consume(data, callback_channels);
                },
                |error| eprintln!("audio: {error}"),
                None,
            )
            .map_err(|error| error.to_string())?;
        stream.play().map_err(|error| error.to_string())?;

        let (commands, receiver) = std::sync::mpsc::channel();
        let mixer_ring = Arc::clone(&ring);
        thread::Builder::new()
            .name("opencut-audio-mix".into())
            .spawn(move || mix_loop(receiver, mixer_ring, Mixer::new(ffmpeg, settings)))
            .map_err(|error| error.to_string())?;

        Ok(Self {
            _stream: stream,
            commands,
            ring,
            sample_rate,
        })
    }

    pub(crate) fn set_clips(&self, clips: Vec<AudioClipPlan>) {
        self.commands.send(AudioCommand::SetClips(clips)).ok();
    }

    /// Restarts the clock at `seconds` right away; the mixer follows.
    pub(crate) fn seek(&self, seconds: f64) {
        self.ring.lock().unwrap_or_else(|e| e.into_inner()).seek(seconds);
        self.commands.send(AudioCommand::Seek(seconds)).ok();
    }

    pub(crate) fn play(&self) {
        self.commands.send(AudioCommand::Play).ok();
    }

    pub(crate) fn pause(&self) {
        self.ring.lock().unwrap_or_else(|e| e.into_inner()).playing = false;
        self.commands.send(AudioCommand::Pause).ok();
    }

    /// Where the device is on the timeline, from the frames it has played
    /// since the last seek; `None` while paused. The picture follows this
    /// so sound and frame cannot drift apart.
    pub(crate) fn position_seconds(&self) -> Option<f64> {
        self.ring
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .position_seconds(self.sample_rate)
    }
}

fn mix_loop(receiver: Receiver<AudioCommand>, ring: Arc<Mutex<Ring>>, mut mixer: Mixer) {
    let settings = mixer.settings();
    let channels = settings.channels as usize;
    // Keep this much audio decoded ahead of the device.
    let target_samples = (settings.sample_rate as usize / 5) * channels;
    let chunk_frames = settings.sample_rate as usize / 50;
    let mut chunk = vec![0f32; chunk_frames * channels];
    let mut playing = false;

    loop {
        // Apply every pending command first.
        loop {
            match receiver.try_recv() {
                Ok(AudioCommand::SetClips(clips)) => mixer.set_clips(clips),
                Ok(AudioCommand::Seek(seconds)) => {
                    mixer.seek(seconds);
                    ring.lock().unwrap_or_else(|e| e.into_inner()).seek(seconds);
                }
                Ok(AudioCommand::Play) => {
                    playing = true;
                    ring.lock().unwrap_or_else(|e| e.into_inner()).playing = true;
                }
                Ok(AudioCommand::Pause) => {
                    playing = false;
                    mixer.stop();
                    let mut ring = ring.lock().unwrap_or_else(|e| e.into_inner());
                    ring.playing = false;
                    ring.samples.clear();
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        if !playing {
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(command) => {
                    // Put it back through the same path on the next turn.
                    match command {
                        AudioCommand::SetClips(clips) => mixer.set_clips(clips),
                        AudioCommand::Seek(seconds) => {
                            mixer.seek(seconds);
                            ring.lock().unwrap_or_else(|e| e.into_inner()).seek(seconds);
                        }
                        AudioCommand::Play => {
                            playing = true;
                            ring.lock().unwrap_or_else(|e| e.into_inner()).playing = true;
                        }
                        AudioCommand::Pause => {}
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
            continue;
        }
        let buffered = ring.lock().unwrap_or_else(|e| e.into_inner()).samples.len();
        if buffered >= target_samples {
            thread::sleep(Duration::from_millis(5));
            continue;
        }
        mixer.fill(&mut chunk);
        let mut ring = ring.lock().unwrap_or_else(|e| e.into_inner());
        ring.samples.extend(chunk.iter().copied());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_clock_counts_played_frames_from_the_last_seek() {
        let mut ring = Ring::new(48_000);
        assert_eq!(ring.position_seconds(48_000), None, "paused: no clock");
        ring.seek(2.0);
        ring.playing = true;
        ring.samples.extend(std::iter::repeat_n(0.5, 4_800 * 2));
        let mut buffer = vec![0f32; 2_400 * 2];
        ring.consume(&mut buffer, 2);
        assert_eq!(buffer[0], 0.5);
        assert!((ring.position_seconds(48_000).unwrap() - 2.05).abs() < 1e-9);
        // Underrun: silence, but time passes.
        let mut big = vec![1f32; 48_000 * 2];
        ring.consume(&mut big, 2);
        assert_eq!(big[big.len() - 1], 0.0);
        assert!((ring.position_seconds(48_000).unwrap() - 3.05).abs() < 1e-9);
        // A seek restarts the count; pausing stops the clock and clears nothing by itself.
        ring.seek(0.25);
        assert!((ring.position_seconds(48_000).unwrap() - 0.25).abs() < 1e-9);
        ring.playing = false;
        ring.consume(&mut buffer, 2);
        assert_eq!(buffer[0], 0.0);
        assert_eq!(ring.position_seconds(48_000), None);
    }
}
