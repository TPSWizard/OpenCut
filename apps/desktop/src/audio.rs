//! Audio for playback: a mixer thread keeps a ring buffer ahead of the
//! device callback, which only copies samples out.

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
}

pub(crate) struct AudioOutput {
    _stream: cpal::Stream,
    commands: Sender<AudioCommand>,
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

        let ring = Arc::new(Mutex::new(Ring {
            samples: VecDeque::with_capacity(sample_rate as usize),
            playing: false,
        }));
        let callback_ring = Arc::clone(&ring);
        let stream = device
            .build_output_stream(
                &stream_config,
                move |data: &mut [f32], _| {
                    let mut ring = callback_ring.lock().unwrap_or_else(|e| e.into_inner());
                    for sample in data.iter_mut() {
                        *sample = if ring.playing {
                            ring.samples.pop_front().unwrap_or(0.0)
                        } else {
                            0.0
                        };
                    }
                },
                |error| eprintln!("audio: {error}"),
                None,
            )
            .map_err(|error| error.to_string())?;
        stream.play().map_err(|error| error.to_string())?;

        let (commands, receiver) = std::sync::mpsc::channel();
        thread::Builder::new()
            .name("opencut-audio-mix".into())
            .spawn(move || mix_loop(receiver, ring, Mixer::new(ffmpeg, settings)))
            .map_err(|error| error.to_string())?;

        Ok(Self {
            _stream: stream,
            commands,
        })
    }

    pub(crate) fn set_clips(&self, clips: Vec<AudioClipPlan>) {
        self.commands.send(AudioCommand::SetClips(clips)).ok();
    }

    pub(crate) fn seek(&self, seconds: f64) {
        self.commands.send(AudioCommand::Seek(seconds)).ok();
    }

    pub(crate) fn play(&self) {
        self.commands.send(AudioCommand::Play).ok();
    }

    pub(crate) fn pause(&self) {
        self.commands.send(AudioCommand::Pause).ok();
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
                    let mut ring = ring.lock().unwrap_or_else(|e| e.into_inner());
                    ring.samples.clear();
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
                            ring.lock().unwrap_or_else(|e| e.into_inner()).samples.clear();
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
