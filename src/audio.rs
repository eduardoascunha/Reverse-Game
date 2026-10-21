use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::config::Config;
use crate::round::Clip;

#[derive(Debug)]
pub struct DeviceInfo {
    pub name: String,
    pub is_default: bool,
}

pub fn select_input(config: &Config) -> Result<cpal::Device> {
    select_device(
        config.input_device.as_deref(),
        cpal::default_host().input_devices(),
        cpal::default_host().default_input_device(),
        "input",
        "microphone",
    )
}

pub fn select_output(config: &Config) -> Result<cpal::Device> {
    select_device(
        config.output_device.as_deref(),
        cpal::default_host().output_devices(),
        cpal::default_host().default_output_device(),
        "output",
        "speaker",
    )
}

pub fn input_devices() -> Result<Vec<DeviceInfo>> {
    devices(
        cpal::default_host().input_devices(),
        cpal::default_host().default_input_device(),
        "input",
    )
}

pub fn output_devices() -> Result<Vec<DeviceInfo>> {
    devices(
        cpal::default_host().output_devices(),
        cpal::default_host().default_output_device(),
        "output",
    )
}

fn select_device(
    configured_name: Option<&str>,
    devices: Result<impl Iterator<Item = cpal::Device>, cpal::Error>,
    default_device: Option<cpal::Device>,
    kind: &str,
    missing_message: &str,
) -> Result<cpal::Device> {
    if let Some(name) = configured_name {
        let device = devices
            .with_context(|| format!("failed to enumerate {kind} devices"))?
            .find(|device| device.to_string() == name);
        return device.ok_or_else(|| {
            anyhow::anyhow!("{kind} device \"{name}\" not found; run with --list-devices")
        });
    }

    default_device.ok_or_else(|| anyhow::anyhow!("no {missing_message} found"))
}

fn devices(
    devices: Result<impl Iterator<Item = cpal::Device>, cpal::Error>,
    default_device: Option<cpal::Device>,
    kind: &str,
) -> Result<Vec<DeviceInfo>> {
    let default_name = default_device.map(|device| device.to_string());

    devices
        .with_context(|| format!("failed to enumerate {kind} devices"))?
        .map(|device| {
            let name = device.to_string();
            Ok(DeviceInfo {
                is_default: default_name.as_deref() == Some(name.as_str()),
                name,
            })
        })
        .collect()
}

pub struct Recorder {
    stream: cpal::Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    pub captured: Arc<AtomicUsize>,
    pub level: Arc<AtomicU32>,
    pub stream_error: Arc<Mutex<Option<String>>>,
}

#[derive(Clone)]
struct CaptureState {
    samples: Arc<Mutex<Vec<f32>>>,
    captured: Arc<AtomicUsize>,
    level: Arc<AtomicU32>,
    stream_error: Arc<Mutex<Option<String>>>,
}

impl Recorder {
    pub fn start(device: &cpal::Device, max_samples: usize) -> Result<Self> {
        let supported_config = device
            .default_input_config()
            .context("failed to get microphone configuration")?;
        let sample_format = supported_config.sample_format();
        let config = supported_config.config();
        let state = CaptureState {
            samples: Arc::new(Mutex::new(Vec::with_capacity(max_samples))),
            captured: Arc::new(AtomicUsize::new(0)),
            level: Arc::new(AtomicU32::new(0)),
            stream_error: Arc::new(Mutex::new(None)),
        };

        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                build_input_stream::<f32>(device, config, max_samples, state.clone(), |sample| {
                    sample
                })
            }
            cpal::SampleFormat::I16 => {
                build_input_stream::<i16>(device, config, max_samples, state.clone(), |sample| {
                    sample as f32 / i16::MAX as f32
                })
            }
            cpal::SampleFormat::U16 => {
                build_input_stream::<u16>(device, config, max_samples, state.clone(), |sample| {
                    sample as f32 / u16::MAX as f32 * 2.0 - 1.0
                })
            }
            format => bail!("unsupported microphone sample format: {format}"),
        }?;

        stream.play().context("failed to start microphone stream")?;

        Ok(Self {
            stream,
            samples: state.samples,
            sample_rate: supported_config.sample_rate(),
            captured: state.captured,
            level: state.level,
            stream_error: state.stream_error,
        })
    }

    pub fn stop(self) -> Clip {
        let Self {
            stream,
            samples,
            sample_rate,
            ..
        } = self;
        drop(stream);

        Clip {
            samples: samples
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone(),
            sample_rate,
        }
    }
}

pub struct Player {
    device: cpal::Device,
    config: cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    stream: Option<cpal::Stream>,
    finished: Arc<AtomicBool>,
    pub stream_error: Arc<Mutex<Option<String>>>,
}

impl Player {
    pub fn new(device: cpal::Device) -> Result<Self> {
        let supported_config = device
            .default_output_config()
            .context("failed to get speaker configuration")?;

        Ok(Self {
            device,
            config: supported_config.config(),
            sample_format: supported_config.sample_format(),
            stream: None,
            finished: Arc::new(AtomicBool::new(true)),
            stream_error: Arc::new(Mutex::new(None)),
        })
    }

    pub fn play(&mut self, clip: &Clip) -> Result<()> {
        self.stop();
        clear_error(&self.stream_error);

        if clip.samples.is_empty() {
            return Ok(());
        }

        let samples = Arc::new(resample(
            &clip.samples,
            clip.sample_rate,
            self.config.sample_rate,
        ));
        let finished = Arc::new(AtomicBool::new(false));
        let stream = match self.sample_format {
            cpal::SampleFormat::F32 => build_output_stream::<f32>(
                &self.device,
                self.config,
                Arc::clone(&samples),
                Arc::clone(&finished),
                Arc::clone(&self.stream_error),
                |sample| sample,
            ),
            cpal::SampleFormat::I16 => build_output_stream::<i16>(
                &self.device,
                self.config,
                Arc::clone(&samples),
                Arc::clone(&finished),
                Arc::clone(&self.stream_error),
                |sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16,
            ),
            cpal::SampleFormat::U16 => build_output_stream::<u16>(
                &self.device,
                self.config,
                Arc::clone(&samples),
                Arc::clone(&finished),
                Arc::clone(&self.stream_error),
                |sample| ((sample.clamp(-1.0, 1.0) + 1.0) * 0.5 * u16::MAX as f32).round() as u16,
            ),
            format => bail!("unsupported speaker sample format: {format}"),
        }?;

        stream.play().context("failed to start speaker stream")?;
        self.finished = finished;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.stream = None;
        self.finished.store(true, Ordering::Release);
    }

    pub fn is_playing(&self) -> bool {
        !self.finished.load(Ordering::Acquire)
    }
}

fn build_input_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    max_samples: usize,
    state: CaptureState,
    to_f32: impl Fn(T) -> f32 + Send + 'static,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample + Copy,
{
    let channels = config.channels as usize;
    let data_state = state.clone();

    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                capture_samples(
                    data,
                    channels,
                    max_samples,
                    &data_state.samples,
                    &data_state.captured,
                    &data_state.level,
                    &to_f32,
                );
            },
            move |error| store_error(&state.stream_error, error),
            None,
        )
        .context("failed to build microphone stream")
}

fn capture_samples<T>(
    data: &[T],
    channels: usize,
    max_samples: usize,
    samples: &Arc<Mutex<Vec<f32>>>,
    captured: &AtomicUsize,
    level: &AtomicU32,
    to_f32: &impl Fn(T) -> f32,
) where
    T: Copy,
{
    let mut samples = samples.lock().unwrap_or_else(|error| error.into_inner());
    let mut peak = 0.0_f32;

    for frame in data.chunks_exact(channels) {
        if samples.len() == max_samples {
            break;
        }

        let sample = frame.iter().copied().map(to_f32).sum::<f32>() / channels as f32;
        peak = peak.max(sample.abs());
        samples.push(sample);
    }

    captured.store(samples.len(), Ordering::Release);
    level.store(peak.to_bits(), Ordering::Release);
}

fn build_output_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    samples: Arc<Vec<f32>>,
    finished: Arc<AtomicBool>,
    stream_error: Arc<Mutex<Option<String>>>,
    from_f32: impl Fn(f32) -> T + Send + 'static,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample + Copy,
{
    let channels = config.channels as usize;
    let position = Arc::new(AtomicUsize::new(0));

    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                fill_output(data, channels, &samples, &position, &finished, &from_f32);
            },
            move |error| store_error(&stream_error, error),
            None,
        )
        .context("failed to build speaker stream")
}

fn fill_output<T: Copy>(
    data: &mut [T],
    channels: usize,
    samples: &[f32],
    position: &AtomicUsize,
    finished: &AtomicBool,
    from_f32: &impl Fn(f32) -> T,
) {
    for frame in data.chunks_exact_mut(channels) {
        let position = position.fetch_add(1, Ordering::Relaxed);
        let sample = match samples.get(position) {
            Some(sample) => *sample,
            None => {
                finished.store(true, Ordering::Release);
                0.0
            }
        };
        frame.fill(from_f32(sample));
    }
}

fn resample(samples: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
    if samples.is_empty() || source_rate == target_rate || source_rate == 0 || target_rate == 0 {
        return samples.to_vec();
    }

    let output_len =
        ((samples.len() as u64 * target_rate as u64 / source_rate as u64) as usize).max(1);
    (0..output_len)
        .map(|index| {
            let source_position = index as f64 * source_rate as f64 / target_rate as f64;
            let lower = source_position.floor() as usize;
            let upper = (lower + 1).min(samples.len() - 1);
            let fraction = (source_position - lower as f64) as f32;
            samples[lower] + (samples[upper] - samples[lower]) * fraction
        })
        .collect()
}

fn clear_error(stream_error: &Mutex<Option<String>>) {
    *stream_error
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = None;
}

fn store_error(stream_error: &Mutex<Option<String>>, error: cpal::Error) {
    *stream_error
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(error.to_string());
}
