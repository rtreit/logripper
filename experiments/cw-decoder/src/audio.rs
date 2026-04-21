//! Audio sources: file decoding (via Symphonia) and live capture (via cpal).

use std::fs::File;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub struct DecodedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// Decode an audio file (mp3/wav/aac/m4a/etc) into a mono f32 PCM buffer.
pub fn decode_file(path: &Path) -> Result<DecodedAudio> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .context("probing audio format")?;
    let mut format = probed.format;

    let track = format
        .default_track()
        .ok_or_else(|| anyhow!("no default audio track"))?;
    let track_id = track.id;
    let codec_params = track.codec_params.clone();

    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .context("creating decoder")?;

    let sample_rate = codec_params
        .sample_rate
        .ok_or_else(|| anyhow!("unknown sample rate"))?;
    let channels = codec_params
        .channels
        .ok_or_else(|| anyhow!("unknown channel layout"))?
        .count();

    let mut samples: Vec<f32> = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(audio_buf) => {
                if sample_buf.is_none() {
                    let spec = *audio_buf.spec();
                    let duration = audio_buf.capacity() as u64;
                    sample_buf = Some(SampleBuffer::<f32>::new(duration, spec));
                }
                if let Some(buf) = sample_buf.as_mut() {
                    buf.copy_interleaved_ref(audio_buf);
                    let interleaved = buf.samples();
                    if channels == 1 {
                        samples.extend_from_slice(interleaved);
                    } else {
                        for frame in interleaved.chunks_exact(channels) {
                            let avg = frame.iter().copied().sum::<f32>() / channels as f32;
                            samples.push(avg);
                        }
                    }
                }
            }
            Err(SymError::DecodeError(_)) => continue,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }
    }

    Ok(DecodedAudio {
        samples,
        sample_rate,
    })
}

// --- Live capture --------------------------------------------------------

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use std::sync::Arc;

pub struct LiveCapture {
    pub sample_rate: u32,
    pub device_name: String,
    /// Rolling ring buffer of the most recent N seconds of mono f32 samples.
    pub buffer: Arc<Mutex<RingBuffer>>,
    _stream: cpal::Stream,
}

pub struct RingBuffer {
    capacity: usize,
    data: Vec<f32>,
    /// Total samples ever written; useful for "have we got fresh data" checks.
    pub written: u64,
}

impl RingBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            data: Vec::with_capacity(capacity),
            written: 0,
        }
    }

    fn push_slice(&mut self, samples: &[f32]) {
        self.written = self.written.saturating_add(samples.len() as u64);
        if samples.len() >= self.capacity {
            let start = samples.len() - self.capacity;
            self.data.clear();
            self.data.extend_from_slice(&samples[start..]);
            return;
        }
        let total = self.data.len() + samples.len();
        if total > self.capacity {
            let drop = total - self.capacity;
            self.data.drain(..drop);
        }
        self.data.extend_from_slice(samples);
    }

    pub fn snapshot(&self) -> Vec<f32> {
        self.data.clone()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }
}

/// Find an input device whose name contains `query` (case-insensitive). When
/// `query` is `None`, the host default input is used.
pub fn open_input(query: Option<&str>, window_seconds: f32) -> Result<LiveCapture> {
    let host = cpal::default_host();

    let device = if let Some(q) = query {
        let q_lower = q.to_lowercase();
        host.input_devices()?
            .find(|d| {
                d.name()
                    .map(|n| n.to_lowercase().contains(&q_lower))
                    .unwrap_or(false)
            })
            .ok_or_else(|| anyhow!("no input device matching {q:?}"))?
    } else {
        host.default_input_device()
            .ok_or_else(|| anyhow!("no default input device"))?
    };

    let device_name = device.name().unwrap_or_else(|_| "<unknown>".to_string());
    let config = device.default_input_config().context("input config")?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let capacity = ((sample_rate as f32 * window_seconds) as usize).max(1);
    let buffer = Arc::new(Mutex::new(RingBuffer::new(capacity)));

    let err_fn = |e| eprintln!("cpal stream error: {e}");
    let buffer_cb = Arc::clone(&buffer);

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _| {
                push_mono(&buffer_cb, data, channels);
            },
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data: &[i16], _| {
                let f: Vec<f32> = data.iter().map(|s| *s as f32 / i16::MAX as f32).collect();
                push_mono(&buffer_cb, &f, channels);
            },
            err_fn,
            None,
        )?,
        cpal::SampleFormat::U16 => device.build_input_stream(
            &config.into(),
            move |data: &[u16], _| {
                let f: Vec<f32> = data
                    .iter()
                    .map(|s| (*s as f32 - 32768.0) / 32768.0)
                    .collect();
                push_mono(&buffer_cb, &f, channels);
            },
            err_fn,
            None,
        )?,
        other => return Err(anyhow!("unsupported sample format: {other:?}")),
    };
    stream.play().context("starting input stream")?;

    Ok(LiveCapture {
        sample_rate,
        device_name,
        buffer,
        _stream: stream,
    })
}

fn push_mono(buf: &Arc<Mutex<RingBuffer>>, data: &[f32], channels: usize) {
    let mut lock = buf.lock();
    if channels == 1 {
        lock.push_slice(data);
    } else {
        let mut mono = Vec::with_capacity(data.len() / channels);
        for frame in data.chunks_exact(channels) {
            let avg = frame.iter().copied().sum::<f32>() / channels as f32;
            mono.push(avg);
        }
        lock.push_slice(&mono);
    }
}

pub fn list_input_devices() -> Result<Vec<String>> {
    let host = cpal::default_host();
    let mut names = Vec::new();
    for d in host.input_devices()? {
        if let Ok(n) = d.name() {
            names.push(n);
        }
    }
    Ok(names)
}
