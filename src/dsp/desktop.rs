use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, Device, SampleFormat, SampleRate, Stream, StreamConfig, SupportedStreamConfig,
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use super::{DspCommand, DspEngine, EngineState};

const RING_CAPACITY: usize = 96_000;
const TARGET_RING_SAMPLES: usize = 1_024;
const TWO_PI: f32 = 6.283_185_5;
static INPUT_LOG_TICK: AtomicU32 = AtomicU32::new(0);
static OUTPUT_LOG_TICK: AtomicU32 = AtomicU32::new(0);

#[derive(Clone)]
struct SharedState {
    ring: VecDeque<f32>,
    input_gain: f32,
    master_gain: f32,
    input_peak: f32,
    callback_ms: f32,
    sample_rate_hz: f32,
    sweep_active: bool,
    sweep_frequency_hz: f32,
    sweep_amplitude: f32,
    sweep_phase: f32,
    underruns: u64,
    trimmed_samples: u64,
    clipped_samples: u64,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            ring: VecDeque::with_capacity(RING_CAPACITY),
            input_gain: 1.0,
            master_gain: 1.0,
            input_peak: 0.0,
            callback_ms: 0.0,
            sample_rate_hz: 48_000.0,
            sweep_active: false,
            sweep_frequency_hz: 1_000.0,
            sweep_amplitude: 0.0,
            sweep_phase: 0.0,
            underruns: 0,
            trimmed_samples: 0,
            clipped_samples: 0,
        }
    }
}

pub struct DesktopEngine {
    state: EngineState,
    shared: Arc<Mutex<SharedState>>,
    input_stream: Option<Stream>,
    output_stream: Option<Stream>,
}

impl DesktopEngine {
    pub fn new() -> Self {
        Self {
            state: EngineState::Stopped,
            shared: Arc::new(Mutex::new(SharedState::default())),
            input_stream: None,
            output_stream: None,
        }
    }

    fn start_streams(&mut self) -> Result<(), String> {
        if self.input_stream.is_some() || self.output_stream.is_some() {
            return Ok(());
        }

        let host = cpal::default_host();
        let input_device =
            pick_input_device(&host).ok_or_else(|| "No usable input device found".to_string())?;
        let output_device =
            pick_output_device(&host).ok_or_else(|| "No usable output device found".to_string())?;
        let input_name = input_device
            .name()
            .unwrap_or_else(|_| "Unknown input device".to_string());
        let output_name = output_device
            .name()
            .unwrap_or_else(|_| "Unknown output device".to_string());

        let output_supported = pick_output_config(&output_device)
            .map_err(|err| format!("Failed to get supported output config: {err}"))?;
        let input_supported =
            pick_input_config_for_rate(&input_device, output_supported.sample_rate().0)
                .map_err(|err| format!("Failed to get supported input config: {err}"))?;
        eprintln!(
            "Desktop audio using input='{}' ({:?}, {}ch @ {}Hz), output='{}' ({:?}, {}ch @ {}Hz)",
            input_name,
            input_supported.sample_format(),
            input_supported.channels(),
            input_supported.sample_rate().0,
            output_name,
            output_supported.sample_format(),
            output_supported.channels(),
            output_supported.sample_rate().0
        );

        let input_config = StreamConfig {
            channels: input_supported.channels(),
            sample_rate: input_supported.sample_rate(),
            buffer_size: BufferSize::Default,
        };
        let output_config = StreamConfig {
            channels: output_supported.channels(),
            sample_rate: output_supported.sample_rate(),
            buffer_size: BufferSize::Default,
        };
        if let Ok(mut guard) = self.shared.lock() {
            guard.sample_rate_hz = output_config.sample_rate.0 as f32;
        }
        if input_config.sample_rate != output_config.sample_rate {
            eprintln!(
                "Desktop audio sample-rate mismatch: input={}Hz output={}Hz (quality may degrade)",
                input_config.sample_rate.0, output_config.sample_rate.0
            );
        }

        let input_channels = input_config.channels as usize;
        let output_channels = output_config.channels as usize;
        let input_shared = Arc::clone(&self.shared);
        let output_shared = Arc::clone(&self.shared);

        let input_err_fn = |err| eprintln!("Desktop input stream error: {err}");
        let output_err_fn = |err| eprintln!("Desktop output stream error: {err}");

        let input_stream = match input_supported.sample_format() {
            SampleFormat::F32 => input_device
                .build_input_stream(
                    &input_config,
                    move |data: &[f32], _| {
                        write_input_samples(data.iter().copied(), input_channels, &input_shared)
                    },
                    input_err_fn,
                    None,
                )
                .map_err(|err| format!("Failed to build f32 input stream: {err}"))?,
            SampleFormat::I16 => input_device
                .build_input_stream(
                    &input_config,
                    move |data: &[i16], _| {
                        write_input_samples(
                            data.iter().map(|sample| *sample as f32 / i16::MAX as f32),
                            input_channels,
                            &input_shared,
                        )
                    },
                    input_err_fn,
                    None,
                )
                .map_err(|err| format!("Failed to build i16 input stream: {err}"))?,
            SampleFormat::U16 => input_device
                .build_input_stream(
                    &input_config,
                    move |data: &[u16], _| {
                        write_input_samples(
                            data.iter()
                                .map(|sample| (*sample as f32 / u16::MAX as f32) * 2.0 - 1.0),
                            input_channels,
                            &input_shared,
                        )
                    },
                    input_err_fn,
                    None,
                )
                .map_err(|err| format!("Failed to build u16 input stream: {err}"))?,
            _ => return Err("Unsupported desktop input sample format".to_string()),
        };

        let output_stream = match output_supported.sample_format() {
            SampleFormat::F32 => output_device
                .build_output_stream(
                    &output_config,
                    move |data: &mut [f32], _| {
                        render_output_f32(data, output_channels, &output_shared)
                    },
                    output_err_fn,
                    None,
                )
                .map_err(|err| format!("Failed to build f32 output stream: {err}"))?,
            SampleFormat::I16 => output_device
                .build_output_stream(
                    &output_config,
                    move |data: &mut [i16], _| {
                        render_output_i16(data, output_channels, &output_shared)
                    },
                    output_err_fn,
                    None,
                )
                .map_err(|err| format!("Failed to build i16 output stream: {err}"))?,
            SampleFormat::U16 => output_device
                .build_output_stream(
                    &output_config,
                    move |data: &mut [u16], _| {
                        render_output_u16(data, output_channels, &output_shared)
                    },
                    output_err_fn,
                    None,
                )
                .map_err(|err| format!("Failed to build u16 output stream: {err}"))?,
            _ => return Err("Unsupported desktop output sample format".to_string()),
        };

        output_stream
            .play()
            .map_err(|err| format!("Failed to start output stream: {err}"))?;
        input_stream
            .play()
            .map_err(|err| format!("Failed to start input stream: {err}"))?;

        self.input_stream = Some(input_stream);
        self.output_stream = Some(output_stream);
        eprintln!("Desktop audio streams started");
        Ok(())
    }

    fn stop_streams(&mut self) {
        self.input_stream = None;
        self.output_stream = None;
        if let Ok(mut shared) = self.shared.lock() {
            shared.ring.clear();
            shared.input_peak = 0.0;
            shared.underruns = 0;
            shared.trimmed_samples = 0;
            shared.clipped_samples = 0;
        }
    }

    pub fn callback_ms(&self) -> f32 {
        if let Ok(shared) = self.shared.lock() {
            shared.callback_ms
        } else {
            0.0
        }
    }

    pub fn input_peak(&self) -> f32 {
        if let Ok(shared) = self.shared.lock() {
            shared.input_peak
        } else {
            0.0
        }
    }

    pub fn underruns(&self) -> u64 {
        if let Ok(shared) = self.shared.lock() {
            shared.underruns
        } else {
            0
        }
    }

    pub fn trimmed_samples(&self) -> u64 {
        if let Ok(shared) = self.shared.lock() {
            shared.trimmed_samples
        } else {
            0
        }
    }

    pub fn clipped_samples(&self) -> u64 {
        if let Ok(shared) = self.shared.lock() {
            shared.clipped_samples
        } else {
            0
        }
    }

    pub fn nr_average_suppression_pct(&self) -> f32 {
        0.0
    }

    pub fn nr_fault_resets(&self) -> u64 {
        0
    }

    pub fn nr_capture_age_seconds(&self) -> f32 {
        0.0
    }
}

impl DspEngine for DesktopEngine {
    fn state(&self) -> EngineState {
        self.state
    }

    fn apply(&mut self, command: DspCommand) {
        match command {
            DspCommand::Start => {
                if let Err(err) = self.start_streams() {
                    eprintln!("Failed to start desktop audio: {err}");
                    self.state = EngineState::Stopped;
                } else {
                    self.state = EngineState::Running;
                }
            }
            DspCommand::Stop => {
                self.stop_streams();
                self.state = EngineState::Stopped;
            }
            DspCommand::SetInputGain(gain) => {
                if let Ok(mut shared) = self.shared.lock() {
                    shared.input_gain = gain.clamp(0.1, 20.0);
                }
            }
            DspCommand::SetMasterGain(gain) => {
                if let Ok(mut shared) = self.shared.lock() {
                    shared.master_gain = gain.clamp(0.2, 4.0);
                }
            }
            DspCommand::StartSweep => {
                if let Err(err) = self.start_streams() {
                    eprintln!("Failed to start desktop sweep audio: {err}");
                    self.state = EngineState::Stopped;
                } else {
                    if let Ok(mut shared) = self.shared.lock() {
                        shared.sweep_active = true;
                        shared.sweep_phase = 0.0;
                    }
                    self.state = EngineState::Running;
                }
            }
            DspCommand::StopSweep => {
                if let Ok(mut shared) = self.shared.lock() {
                    shared.sweep_active = false;
                    shared.sweep_amplitude = 0.0;
                }
            }
            DspCommand::AdvanceSweep(step) => {
                if let Ok(mut shared) = self.shared.lock() {
                    shared.sweep_frequency_hz = step.frequency_hz as f32;
                    shared.sweep_amplitude = step.amplitude.clamp(0.0, 1.0);
                }
            }
            DspCommand::SetNoiseCancel(_)
            | DspCommand::SetNoiseStrength(_)
            | DspCommand::SetNoiseProfileMode(_)
            | DspCommand::CaptureNoiseProfile
            | DspCommand::SetLimiterEnabled(_)
            | DspCommand::SetBandGains { .. }
            | DspCommand::SetSafeMode(_)
            | DspCommand::SetAgcEnabled(_)
            | DspCommand::SetAgcMaxGain(_)
            | DspCommand::SetLowCutHz(_)
            | DspCommand::SetHighCutHz(_)
            | DspCommand::SetEqBand { .. }
            | DspCommand::SetAndroidAudioConfig { .. }
            | DspCommand::SetAndroidPreferredDevices { .. } => {}
        }
    }
}

impl Drop for DesktopEngine {
    fn drop(&mut self) {
        self.stop_streams();
    }
}

fn is_supported_sample_format(format: SampleFormat) -> bool {
    matches!(
        format,
        SampleFormat::F32 | SampleFormat::I16 | SampleFormat::U16
    )
}

fn pick_input_device(host: &cpal::Host) -> Option<Device> {
    if let Some(device) = host.default_input_device() {
        return Some(device);
    }
    host.input_devices().ok()?.next()
}

fn pick_output_device(host: &cpal::Host) -> Option<Device> {
    if let Some(device) = host.default_output_device() {
        return Some(device);
    }
    host.output_devices().ok()?.next()
}

fn pick_input_config_for_rate(
    device: &Device,
    preferred_rate_hz: u32,
) -> Result<SupportedStreamConfig, String> {
    let preferred_rate = SampleRate(preferred_rate_hz);
    if let Ok(default) = device.default_input_config() {
        if is_supported_sample_format(default.sample_format())
            && default.sample_rate().0 == preferred_rate_hz
        {
            return Ok(default);
        }
    }
    let configs = device
        .supported_input_configs()
        .map_err(|err| format!("{err}"))?;
    let ranges: Vec<_> = configs.collect();

    if let Some(matched) = ranges
        .iter()
        .filter(|range| is_supported_sample_format(range.sample_format()))
        .find_map(|range| {
            if range.min_sample_rate() <= preferred_rate
                && preferred_rate <= range.max_sample_rate()
            {
                Some(range.with_sample_rate(preferred_rate))
            } else {
                None
            }
        })
    {
        return Ok(matched);
    }

    if let Ok(default) = device.default_input_config() {
        if is_supported_sample_format(default.sample_format()) {
            return Ok(default);
        }
    }

    ranges
        .into_iter()
        .find_map(|range| {
            if is_supported_sample_format(range.sample_format()) {
                Some(range.with_max_sample_rate())
            } else {
                None
            }
        })
        .ok_or_else(|| "No supported input sample format (f32/i16/u16)".to_string())
}

fn pick_output_config(device: &Device) -> Result<SupportedStreamConfig, String> {
    if let Ok(default) = device.default_output_config() {
        if is_supported_sample_format(default.sample_format()) {
            return Ok(default);
        }
    }
    let mut configs = device
        .supported_output_configs()
        .map_err(|err| format!("{err}"))?;
    configs
        .find_map(|range| {
            if is_supported_sample_format(range.sample_format()) {
                Some(range.with_max_sample_rate())
            } else {
                None
            }
        })
        .ok_or_else(|| "No supported output sample format (f32/i16/u16)".to_string())
}

fn write_input_samples<I>(samples: I, channels: usize, shared: &Arc<Mutex<SharedState>>)
where
    I: Iterator<Item = f32>,
{
    if channels == 0 {
        return;
    }

    let mut peak = 0.0f32;
    if let Ok(mut guard) = shared.lock() {
        let gain = guard.input_gain;
        let mut frame_sum = 0.0f32;
        let mut frame_count = 0usize;
        for sample in samples {
            frame_sum += sample;
            frame_count += 1;
            if frame_count < channels {
                continue;
            }
            let mono = frame_sum / channels as f32;
            let amplified = mono * gain;
            peak = peak.max(amplified.abs());
            if guard.ring.len() >= RING_CAPACITY {
                let _ = guard.ring.pop_front();
            }
            guard.ring.push_back(amplified);
            frame_sum = 0.0;
            frame_count = 0;
        }
        guard.input_peak = 0.9 * guard.input_peak + 0.1 * peak;
        let tick = INPUT_LOG_TICK.fetch_add(1, Ordering::Relaxed);
        if tick.is_multiple_of(60) {
            eprintln!(
                "Desktop input peak={:.4} ring_len={}",
                guard.input_peak,
                guard.ring.len()
            );
        }
    }
}

fn render_output_f32(data: &mut [f32], channels: usize, shared: &Arc<Mutex<SharedState>>) {
    let start = Instant::now();
    if channels == 0 {
        return;
    }
    if let Ok(mut guard) = shared.lock() {
        let mut trimmed = 0usize;
        while guard.ring.len() > TARGET_RING_SAMPLES * 2 {
            let _ = guard.ring.pop_front();
            trimmed += 1;
        }
        guard.trimmed_samples = guard.trimmed_samples.saturating_add(trimmed as u64);
        let mut underruns = 0usize;
        let mut clipped = 0u64;
        for frame in data.chunks_mut(channels) {
            let sample = next_output_sample(&mut guard, &mut underruns);
            let sample = if sample.abs() > 1.0 {
                clipped = clipped.saturating_add(1);
                sample.clamp(-1.0, 1.0)
            } else {
                sample
            };
            for channel_sample in frame.iter_mut() {
                *channel_sample = sample;
            }
        }
        guard.underruns = guard.underruns.saturating_add(underruns as u64);
        guard.clipped_samples = guard.clipped_samples.saturating_add(clipped);
        let elapsed = start.elapsed().as_secs_f32() * 1000.0;
        guard.callback_ms = 0.9 * guard.callback_ms + 0.1 * elapsed;
        let tick = OUTPUT_LOG_TICK.fetch_add(1, Ordering::Relaxed);
        if tick.is_multiple_of(60) {
            eprintln!(
                "Desktop output callback_ms={:.3} underruns={} trimmed={} ring_len={}",
                guard.callback_ms,
                underruns,
                trimmed,
                guard.ring.len()
            );
        }
    }
}

fn render_output_i16(data: &mut [i16], channels: usize, shared: &Arc<Mutex<SharedState>>) {
    let start = Instant::now();
    if channels == 0 {
        return;
    }
    if let Ok(mut guard) = shared.lock() {
        let mut trimmed = 0usize;
        while guard.ring.len() > TARGET_RING_SAMPLES * 2 {
            let _ = guard.ring.pop_front();
            trimmed += 1;
        }
        guard.trimmed_samples = guard.trimmed_samples.saturating_add(trimmed as u64);
        let mut underruns = 0usize;
        let mut clipped = 0u64;
        for frame in data.chunks_mut(channels) {
            let sample = next_output_sample(&mut guard, &mut underruns);
            let sample = if sample.abs() > 1.0 {
                clipped = clipped.saturating_add(1);
                sample.clamp(-1.0, 1.0)
            } else {
                sample
            };
            let encoded = (sample * i16::MAX as f32) as i16;
            for channel_sample in frame.iter_mut() {
                *channel_sample = encoded;
            }
        }
        guard.underruns = guard.underruns.saturating_add(underruns as u64);
        guard.clipped_samples = guard.clipped_samples.saturating_add(clipped);
        let elapsed = start.elapsed().as_secs_f32() * 1000.0;
        guard.callback_ms = 0.9 * guard.callback_ms + 0.1 * elapsed;
        let tick = OUTPUT_LOG_TICK.fetch_add(1, Ordering::Relaxed);
        if tick.is_multiple_of(60) {
            eprintln!(
                "Desktop output callback_ms={:.3} underruns={} trimmed={} ring_len={}",
                guard.callback_ms,
                underruns,
                trimmed,
                guard.ring.len()
            );
        }
    }
}

fn render_output_u16(data: &mut [u16], channels: usize, shared: &Arc<Mutex<SharedState>>) {
    let start = Instant::now();
    if channels == 0 {
        return;
    }
    if let Ok(mut guard) = shared.lock() {
        let mut trimmed = 0usize;
        while guard.ring.len() > TARGET_RING_SAMPLES * 2 {
            let _ = guard.ring.pop_front();
            trimmed += 1;
        }
        guard.trimmed_samples = guard.trimmed_samples.saturating_add(trimmed as u64);
        let mut underruns = 0usize;
        let mut clipped = 0u64;
        for frame in data.chunks_mut(channels) {
            let sample = next_output_sample(&mut guard, &mut underruns);
            let sample = if sample.abs() > 1.0 {
                clipped = clipped.saturating_add(1);
                sample.clamp(-1.0, 1.0)
            } else {
                sample
            };
            let normalized = ((sample + 1.0) * 0.5).clamp(0.0, 1.0);
            let encoded = (normalized * u16::MAX as f32) as u16;
            for channel_sample in frame.iter_mut() {
                *channel_sample = encoded;
            }
        }
        guard.underruns = guard.underruns.saturating_add(underruns as u64);
        guard.clipped_samples = guard.clipped_samples.saturating_add(clipped);
        let elapsed = start.elapsed().as_secs_f32() * 1000.0;
        guard.callback_ms = 0.9 * guard.callback_ms + 0.1 * elapsed;
        let tick = OUTPUT_LOG_TICK.fetch_add(1, Ordering::Relaxed);
        if tick.is_multiple_of(60) {
            eprintln!(
                "Desktop output callback_ms={:.3} underruns={} trimmed={} ring_len={}",
                guard.callback_ms,
                underruns,
                trimmed,
                guard.ring.len()
            );
        }
    }
}

fn next_output_sample(guard: &mut SharedState, underruns: &mut usize) -> f32 {
    let input = match guard.ring.pop_front() {
        Some(value) => value,
        None => {
            *underruns += 1;
            0.0
        }
    };
    let tone = if guard.sweep_active && guard.sweep_amplitude > 0.0 {
        let sample = guard.sweep_phase.sin() * guard.sweep_amplitude;
        let phase_inc = TWO_PI * guard.sweep_frequency_hz / guard.sample_rate_hz.max(1.0);
        guard.sweep_phase += phase_inc;
        if guard.sweep_phase > TWO_PI {
            guard.sweep_phase -= TWO_PI;
        }
        sample
    } else {
        0.0
    };
    (input * guard.master_gain) + tone
}
