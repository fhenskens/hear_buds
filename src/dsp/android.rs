use dioxus::prelude::{SyncSignal, WritableExt};
use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex, OnceLock};

use super::{DspCommand, DspEngine, EngineState, SweepStep};
use crate::android_audio;
use crate::android_service;
use hear_buds_dsp::{
    hear_buds_dsp_capture_noise_profile, hear_buds_dsp_create, hear_buds_dsp_destroy,
    hear_buds_dsp_get_nr_avg_suppression_pct, hear_buds_dsp_get_nr_capture_age_seconds,
    hear_buds_dsp_get_nr_fault_resets, hear_buds_dsp_get_perf_level, hear_buds_dsp_process,
    hear_buds_dsp_set_agc_enabled, hear_buds_dsp_set_agc_max_gain, hear_buds_dsp_set_band_gains,
    hear_buds_dsp_set_eq_band, hear_buds_dsp_set_high_cut_hz, hear_buds_dsp_set_limiter_enabled,
    hear_buds_dsp_set_low_cut_hz, hear_buds_dsp_set_master_gain, hear_buds_dsp_set_noise_cancel,
    hear_buds_dsp_set_noise_profile_mode, hear_buds_dsp_set_noise_strength,
    hear_buds_dsp_set_safe_mode, DspHandle,
};
use trombone::backend::android::{AndroidBackend, AndroidBackendKind};
use trombone::backend::AudioBackend;
use trombone::core::callback::CallbackInfo;
use trombone::core::config::{
    ContentType, Direction, PerformanceMode, SampleFormat, SharingMode, StreamConfig,
    StreamOptions, Usage,
};
use trombone::core::stream::Stream;

const RING_CAPACITY: usize = 96_000;
const TARGET_RING_SAMPLES: usize = 1024;
const TWO_PI: f32 = 6.283_185_5;

static PERF_SINK: OnceLock<Mutex<Option<SyncSignal<f32>>>> = OnceLock::new();
static ROUTE_SINK: OnceLock<Mutex<Option<SyncSignal<String>>>> = OnceLock::new();

fn set_perf_level(level: f32) {
    if let Some(lock) = PERF_SINK.get() {
        if let Ok(mut guard) = lock.lock() {
            if let Some(signal) = guard.as_mut() {
                signal.set(level);
            }
        }
    }
}

fn set_route_status(status: String) {
    if let Some(lock) = ROUTE_SINK.get() {
        if let Ok(mut guard) = lock.lock() {
            if let Some(signal) = guard.as_mut() {
                signal.set(status);
            }
        }
    }
}

#[derive(Default)]
struct SharedState {
    ring: VecDeque<f32>,
    input_gain: f32,
    input_peak: f32,
    callback_ms: f32,
    sample_rate_hz: f32,
    sweep_active: bool,
    sweep_frequency_hz: f32,
    sweep_amplitude: f32,
    sweep_phase: f32,
    last_callback_ns: Option<u64>,
    underruns: u64,
    trimmed_samples: u64,
    clipped_samples: u64,
}

/// Android audio backend via trombone-audio (AAudio/OpenSL ES) and Rust DSP core.
pub struct AndroidEngine {
    state: EngineState,
    shared: Arc<Mutex<SharedState>>,
    input_stream: Option<Stream>,
    output_stream: Option<Stream>,
    dsp: *mut DspHandle,
    backend_kind: AndroidBackendKind,
    frames_per_burst: u32,
    forced_input_device_id: i32,
    forced_output_device_id: i32,
}

impl AndroidEngine {
    pub fn new() -> Self {
        let dsp = hear_buds_dsp_create();
        Self {
            state: EngineState::Stopped,
            shared: Arc::new(Mutex::new(SharedState {
                input_gain: 1.0,
                sample_rate_hz: 48_000.0,
                sweep_frequency_hz: 1000.0,
                ..SharedState::default()
            })),
            input_stream: None,
            output_stream: None,
            dsp,
            backend_kind: AndroidBackendKind::Auto,
            frames_per_burst: 192,
            forced_input_device_id: 0,
            forced_output_device_id: 0,
        }
    }

    fn configure_audio_routing(&mut self) -> bool {
        android_audio::deactivate_bluetooth_input();
        let mut voice_comm = false;
        let mut input_route = "System default mic".to_string();
        if self.forced_input_device_id > 0 {
            if let Some((device_id, is_voice_comm)) =
                android_audio::activate_input_device(self.forced_input_device_id)
            {
                voice_comm = is_voice_comm;
                input_route = android_audio::describe_input_device(device_id)
                    .unwrap_or_else(|| format!("Selected input id={device_id}"));
            }
        } else if let Some(device_id) = android_audio::preferred_input_device_id() {
            input_route = android_audio::describe_input_device(device_id)
                .unwrap_or_else(|| format!("Preferred input id={device_id}"));
        }

        let mut output_route = "Handset speaker".to_string();
        let mut output_voice_comm = false;
        if self.forced_output_device_id > 0 {
            if let Some((device_id, device_voice_comm)) =
                android_audio::activate_output_device(self.forced_output_device_id, voice_comm)
            {
                output_voice_comm = device_voice_comm || voice_comm;
                output_route = android_audio::describe_output_device(device_id)
                    .unwrap_or_else(|| format!("Selected output id={device_id}"));
            }
        } else if let Some((device_id, device_voice_comm)) =
            android_audio::activate_bluetooth_output(voice_comm)
                .or_else(android_audio::preferred_output_device)
        {
            output_voice_comm = device_voice_comm || voice_comm;
            output_route = android_audio::describe_output_device(device_id)
                .unwrap_or_else(|| format!("Bluetooth output id={device_id}"));
        } else if voice_comm {
            output_voice_comm = true;
            output_route = "Voice comm fallback output".to_string();
        }

        let route_summary = format!(
            "Input: {input_route} | Output: {output_route} | VoiceComm: {}",
            if output_voice_comm { "On" } else { "Off" }
        );
        if !output_voice_comm {
            android_audio::deactivate_bluetooth_input();
        }
        eprintln!("Android routing decision: {route_summary}");
        set_route_status(route_summary);
        output_voice_comm
    }

    fn start_streams(&mut self) -> Result<(), String> {
        if self.input_stream.is_some() || self.output_stream.is_some() {
            return Ok(());
        }
        if self.dsp.is_null() {
            return Err("Rust DSP handle was not created".to_string());
        }

        let use_voice_comm_stream = self.configure_audio_routing();

        let backend_kind = self.backend_kind;
        let backend = AndroidBackend::new(backend_kind);
        let mono = NonZeroU32::new(1).expect("literal is non-zero");
        let sample_rate = NonZeroU32::new(48_000).expect("literal is non-zero");
        let burst_value = self.frames_per_burst.clamp(64, 1024);
        let burst = NonZeroU32::new(burst_value).expect("burst is clamped to non-zero");
        let (usage, content_type) = if use_voice_comm_stream {
            (Usage::VoiceCommunication, ContentType::Speech)
        } else {
            (Usage::Media, ContentType::Music)
        };
        let input_options = StreamOptions {
            performance_mode: PerformanceMode::LowLatency,
            sharing_mode: SharingMode::Shared,
            usage,
            content_type,
        };
        let output_options = StreamOptions {
            performance_mode: PerformanceMode::LowLatency,
            sharing_mode: SharingMode::Shared,
            usage,
            content_type,
        };
        let input_config = StreamConfig {
            channels: mono,
            sample_rate_hz: sample_rate,
            frames_per_burst: burst,
            format: SampleFormat::F32,
            direction: Direction::Input,
            options: input_options,
        };
        let output_config = StreamConfig {
            channels: mono,
            sample_rate_hz: sample_rate,
            frames_per_burst: burst,
            format: SampleFormat::F32,
            direction: Direction::Output,
            options: output_options,
        };

        let mut input_stream = backend
            .create_stream(input_config)
            .map_err(|err| format!("Failed to create trombone input stream: {err:?}"))?;
        let mut output_stream = backend
            .create_stream(output_config)
            .map_err(|err| format!("Failed to create trombone output stream: {err:?}"))?;
        eprintln!(
            "Android trombone config: backend={} requested_burst={} usage={}",
            match backend_kind {
                AndroidBackendKind::Auto => "auto",
                AndroidBackendKind::AAudio => "aaudio",
                AndroidBackendKind::OpenSLES => "opensl",
            },
            burst_value,
            if use_voice_comm_stream {
                "voice-communication"
            } else {
                "media"
            }
        );

        let input_channels = input_stream.config().channels.get() as usize;
        let output_channels = output_stream.config().channels.get() as usize;
        let output_rate = output_stream.config().sample_rate_hz.get() as f32;
        if let Ok(mut guard) = self.shared.lock() {
            guard.sample_rate_hz = output_rate;
            guard.last_callback_ns = None;
            guard.underruns = 0;
            guard.trimmed_samples = 0;
            guard.clipped_samples = 0;
        }

        let input_shared = Arc::clone(&self.shared);
        input_stream
            .set_capture_callback(move |_info, input: &[f32]| {
                if input_channels == 0 {
                    return;
                }
                if let Ok(mut guard) = input_shared.lock() {
                    let mut peak = 0.0f32;
                    let gain = guard.input_gain;
                    let mut frame_sum = 0.0f32;
                    let mut frame_count = 0usize;
                    for sample in input {
                        frame_sum += *sample;
                        frame_count += 1;
                        if frame_count < input_channels {
                            continue;
                        }
                        let mono = frame_sum / input_channels as f32;
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
                }
            })
            .map_err(|err| format!("Failed to set trombone input callback: {err:?}"))?;

        let output_shared = Arc::clone(&self.shared);
        let dsp_addr = self.dsp as usize;
        output_stream
            .set_render_callback(move |info: CallbackInfo, out: &mut [f32]| {
                if output_channels == 0 {
                    out.fill(0.0);
                    return;
                }
                let dsp_ptr = dsp_addr as *mut DspHandle;
                if let Ok(mut guard) = output_shared.lock() {
                    let sweep_only = guard.sweep_active && guard.sweep_amplitude > 0.0;
                    let mut trimmed = 0u64;
                    while guard.ring.len() > TARGET_RING_SAMPLES * 2 {
                        let _ = guard.ring.pop_front();
                        trimmed += 1;
                    }
                    guard.trimmed_samples = guard.trimmed_samples.saturating_add(trimmed);
                    for frame in out.chunks_mut(output_channels) {
                        let input = if sweep_only {
                            0.0
                        } else {
                            match guard.ring.pop_front() {
                                Some(value) => value,
                                None => {
                                    guard.underruns = guard.underruns.saturating_add(1);
                                    0.0
                                }
                            }
                        };
                        let tone = if sweep_only {
                            let sample = guard.sweep_phase.sin() * guard.sweep_amplitude;
                            let phase_inc =
                                TWO_PI * guard.sweep_frequency_hz / guard.sample_rate_hz.max(1.0);
                            guard.sweep_phase += phase_inc;
                            if guard.sweep_phase > TWO_PI {
                                guard.sweep_phase -= TWO_PI;
                            }
                            sample
                        } else {
                            0.0
                        };
                        let sample = (input + tone).clamp(-1.0, 1.0);
                        frame[0] = sample;
                        for channel in frame.iter_mut().skip(1) {
                            *channel = sample;
                        }
                    }

                    unsafe {
                        if !dsp_ptr.is_null() && !sweep_only {
                            hear_buds_dsp_process(
                                dsp_ptr,
                                out.as_mut_ptr(),
                                out.len() as i32,
                                guard.sample_rate_hz,
                            );
                            set_perf_level(hear_buds_dsp_get_perf_level(dsp_ptr));
                        }
                    }
                    for sample in out.iter_mut() {
                        if sample.abs() > 1.0 {
                            guard.clipped_samples = guard.clipped_samples.saturating_add(1);
                            *sample = sample.clamp(-1.0, 1.0);
                        }
                    }

                    if let Some(previous_ns) = guard.last_callback_ns {
                        let delta_ns = info.callback_time_ns.saturating_sub(previous_ns);
                        let delta_ms = delta_ns as f32 / 1_000_000.0;
                        if delta_ms > 0.0 {
                            guard.callback_ms = 0.9 * guard.callback_ms + 0.1 * delta_ms;
                        }
                    }
                    guard.last_callback_ns = Some(info.callback_time_ns);
                } else {
                    out.fill(0.0);
                }
            })
            .map_err(|err| format!("Failed to set trombone output callback: {err:?}"))?;

        output_stream
            .start()
            .map_err(|err| format!("Failed to start trombone output stream: {err:?}"))?;
        input_stream
            .start()
            .map_err(|err| format!("Failed to start trombone input stream: {err:?}"))?;

        self.input_stream = Some(input_stream);
        self.output_stream = Some(output_stream);
        Ok(())
    }

    fn stop_streams(&mut self) {
        if let Some(mut input) = self.input_stream.take() {
            let _ = input.stop();
        }
        if let Some(mut output) = self.output_stream.take() {
            let _ = output.stop();
        }
        android_audio::deactivate_bluetooth_input();
        if let Ok(mut shared) = self.shared.lock() {
            shared.ring.clear();
            shared.input_peak = 0.0;
            shared.callback_ms = 0.0;
            shared.sweep_active = false;
            shared.sweep_amplitude = 0.0;
            shared.last_callback_ns = None;
        }
    }

    fn handle_sweep_step(&mut self, step: SweepStep) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.sweep_frequency_hz = step.frequency_hz as f32;
            shared.sweep_amplitude = step.amplitude.clamp(0.0, 1.0);
        }
    }

    pub fn attach_perf_signal(signal: SyncSignal<f32>) {
        let sink = PERF_SINK.get_or_init(|| Mutex::new(None));
        if let Ok(mut guard) = sink.lock() {
            *guard = Some(signal);
        }
    }

    pub fn attach_route_signal(signal: SyncSignal<String>) {
        let sink = ROUTE_SINK.get_or_init(|| Mutex::new(None));
        if let Ok(mut guard) = sink.lock() {
            *guard = Some(signal);
        }
    }
}

impl DspEngine for AndroidEngine {
    fn state(&self) -> EngineState {
        self.state
    }

    fn apply(&mut self, command: DspCommand) {
        match command {
            DspCommand::Start => {
                android_audio::set_communication_mode(true);
                if let Err(err) = self.start_streams() {
                    eprintln!("Failed to start Android trombone audio: {err}");
                    let _ = android_service::set_dsp_foreground_service_enabled(false);
                    android_audio::set_communication_mode(false);
                    self.state = EngineState::Stopped;
                } else {
                    if let Ok(mut shared) = self.shared.lock() {
                        shared.sweep_active = false;
                        shared.sweep_amplitude = 0.0;
                    }
                    android_service::request_ignore_battery_optimizations_if_needed();
                    if !android_service::set_dsp_foreground_service_enabled(true) {
                        eprintln!("Failed to enable foreground DSP service");
                    }
                    self.state = EngineState::Running;
                }
            }
            DspCommand::Stop => {
                self.stop_streams();
                let _ = android_service::set_dsp_foreground_service_enabled(false);
                android_audio::deactivate_bluetooth_input();
                android_audio::set_communication_mode(false);
                set_route_status("Input: Stopped | Output: Stopped | VoiceComm: Off".to_string());
                self.state = EngineState::Stopped;
            }
            DspCommand::SetNoiseCancel(enabled) => unsafe {
                hear_buds_dsp_set_noise_cancel(self.dsp, enabled);
            },
            DspCommand::SetNoiseStrength(strength) => unsafe {
                hear_buds_dsp_set_noise_strength(self.dsp, strength);
            },
            DspCommand::SetNoiseProfileMode(mode) => unsafe {
                hear_buds_dsp_set_noise_profile_mode(self.dsp, mode);
            },
            DspCommand::CaptureNoiseProfile => unsafe {
                hear_buds_dsp_capture_noise_profile(self.dsp);
            },
            DspCommand::SetLimiterEnabled(enabled) => unsafe {
                hear_buds_dsp_set_limiter_enabled(self.dsp, enabled);
            },
            DspCommand::SetBandGains { low, mid, high } => unsafe {
                hear_buds_dsp_set_band_gains(self.dsp, low, mid, high);
            },
            DspCommand::SetSafeMode(enabled) => unsafe {
                hear_buds_dsp_set_safe_mode(self.dsp, enabled);
            },
            DspCommand::SetMasterGain(gain) => unsafe {
                hear_buds_dsp_set_master_gain(self.dsp, gain);
            },
            DspCommand::SetInputGain(gain) => {
                if let Ok(mut shared) = self.shared.lock() {
                    shared.input_gain = gain.clamp(0.1, 20.0);
                }
            }
            DspCommand::SetAgcEnabled(enabled) => unsafe {
                hear_buds_dsp_set_agc_enabled(self.dsp, enabled);
            },
            DspCommand::SetAgcMaxGain(gain) => unsafe {
                hear_buds_dsp_set_agc_max_gain(self.dsp, gain);
            },
            DspCommand::SetLowCutHz(hz) => unsafe {
                hear_buds_dsp_set_low_cut_hz(self.dsp, hz);
            },
            DspCommand::SetHighCutHz(hz) => unsafe {
                hear_buds_dsp_set_high_cut_hz(self.dsp, hz);
            },
            DspCommand::SetEqBand { index, value_db } => unsafe {
                hear_buds_dsp_set_eq_band(self.dsp, index as i32, value_db);
            },
            DspCommand::SetAndroidAudioConfig {
                backend,
                frames_per_burst,
            } => {
                self.backend_kind = match backend {
                    1 => AndroidBackendKind::AAudio,
                    2 => AndroidBackendKind::OpenSLES,
                    _ => AndroidBackendKind::Auto,
                };
                self.frames_per_burst = frames_per_burst.clamp(64, 1024);
                if self.state == EngineState::Running {
                    self.stop_streams();
                    android_audio::set_communication_mode(true);
                    if let Err(err) = self.start_streams() {
                        eprintln!("Failed to reconfigure Android trombone audio: {err}");
                        let _ = android_service::set_dsp_foreground_service_enabled(false);
                        android_audio::set_communication_mode(false);
                        self.state = EngineState::Stopped;
                    }
                }
            }
            DspCommand::SetAndroidPreferredDevices {
                input_device_id,
                output_device_id,
            } => {
                self.forced_input_device_id = input_device_id.max(0);
                self.forced_output_device_id = output_device_id.max(0);
                if self.state == EngineState::Running {
                    self.stop_streams();
                    android_audio::set_communication_mode(true);
                    if let Err(err) = self.start_streams() {
                        eprintln!("Failed to apply Android preferred devices: {err}");
                        let _ = android_service::set_dsp_foreground_service_enabled(false);
                        android_audio::set_communication_mode(false);
                        self.state = EngineState::Stopped;
                    }
                }
            }
            DspCommand::StartSweep => {
                if let Err(err) = self.start_streams() {
                    eprintln!("Failed to start Android trombone sweep audio: {err}");
                    self.state = EngineState::Stopped;
                } else {
                    if let Ok(mut shared) = self.shared.lock() {
                        shared.sweep_active = true;
                        shared.sweep_phase = 0.0;
                        if shared.sweep_amplitude <= 0.0 {
                            shared.sweep_frequency_hz = 1000.0;
                            shared.sweep_amplitude = 0.2;
                        }
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
                self.handle_sweep_step(step);
            }
        }
    }
}

impl AndroidEngine {
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
        if self.dsp.is_null() {
            return 0.0;
        }
        unsafe { hear_buds_dsp_get_nr_avg_suppression_pct(self.dsp) }
    }

    pub fn nr_fault_resets(&self) -> u64 {
        if self.dsp.is_null() {
            return 0;
        }
        unsafe { hear_buds_dsp_get_nr_fault_resets(self.dsp) as u64 }
    }

    pub fn nr_capture_age_seconds(&self) -> f32 {
        if self.dsp.is_null() {
            return 0.0;
        }
        unsafe { hear_buds_dsp_get_nr_capture_age_seconds(self.dsp) }
    }
}

impl Drop for AndroidEngine {
    fn drop(&mut self) {
        self.stop_streams();
        let _ = android_service::set_dsp_foreground_service_enabled(false);
        android_audio::set_communication_mode(false);
        if !self.dsp.is_null() {
            unsafe { hear_buds_dsp_destroy(self.dsp) };
            self.dsp = core::ptr::null_mut();
        }
    }
}
