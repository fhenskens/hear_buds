#![allow(clippy::not_unsafe_ptr_arg_deref)]

use rustfft::{num_complex::Complex32, Fft, FftPlanner};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

const EQ_BANDS: [f32; 7] = [125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0];
const TWO_PI: f32 = 6.283_185_5;
const NOISE_CAPTURE_SEC: f32 = 2.0;
const SPECTRAL_NR_FRAME_SIZE: usize = 256;
const SPECTRAL_NR_HOP_SIZE: usize = 128;
const SPECTRAL_NR_INIT_NOISE: f32 = 1.0e-6;
const SPECTRAL_NR_EPS: f32 = 1.0e-8;
const SPECTRAL_NR_ADAPT_ALPHA: f32 = 0.995;
const SPECTRAL_NR_CAPTURE_ALPHA: f32 = 0.75;
const SPECTRAL_NR_GAIN_STEP_LIMIT_COMFORT: f32 = 0.05;
const SPECTRAL_NR_GAIN_STEP_LIMIT_STRONG: f32 = 0.03;
const SPECTRAL_NR_FAULT_IN_LEVEL: f32 = 5.0e-4;
const SPECTRAL_NR_FAULT_OUT_RATIO: f32 = 1.0e-3;
const SPECTRAL_NR_FAULT_LIMIT: u8 = 3;
const SPECTRAL_NR_SUPPRESSION_SMOOTH: f32 = 0.12;
const PERF_WINDOW_SEC: f32 = 0.5;
const BIQUAD_Q: f32 = 1.0;
const LIMITER_THRESHOLD: f32 = 0.85;
const MAX_BAND_GAIN: f32 = 2.5;
const CROSSOVER_LOW_HZ: f32 = 500.0;
const CROSSOVER_HIGH_HZ: f32 = 2000.0;
const COMP_ATTACK: f32 = 0.02;
const COMP_RELEASE: f32 = 0.002;
const COMP_RATIO: f32 = 2.0;
const COMP_THRESHOLD_DB: f32 = -35.0;
const SAFE_MODE_GAIN: f32 = 0.8;
const MASTER_GAIN_MIN: f32 = 0.5;
const MASTER_GAIN_MAX: f32 = 6.0;
const AGC_ATTACK: f32 = 0.05;
const AGC_RELEASE: f32 = 0.002;
const AGC_TARGET: f32 = 0.08;
const AGC_GAIN_MIN: f32 = 0.25;
const AGC_SILENCE_LEVEL: f32 = 0.01;
const AGC_MAX_GAIN_MIN: f32 = 1.0;
const AGC_MAX_GAIN_MAX: f32 = 20.0;
const LOW_CUT_MIN_HZ: f32 = 20.0;
const LOW_CUT_MAX_HZ: f32 = 400.0;
const HIGH_CUT_MIN_HZ: f32 = 2_000.0;
const HIGH_CUT_MAX_HZ: f32 = 12_000.0;
const DEFAULT_LOW_CUT_HZ: f32 = 20.0;
const DEFAULT_HIGH_CUT_HZ: f32 = 6_600.0;
const FEEDBACK_RESONATOR_DAMPING: f32 = 0.997;
const FEEDBACK_ENV_ATTACK: f32 = 0.08;
const FEEDBACK_ENV_RELEASE: f32 = 0.004;
const FEEDBACK_SELECT_INTERVAL: usize = 128;
const FEEDBACK_RATIO_THRESHOLD: f32 = 5.0;
const FEEDBACK_NOTCH_Q: f32 = 18.0;
const FEEDBACK_MAX_NOTCH_MIX: f32 = 0.55;
const FEEDBACK_NOTCH_ATTACK: f32 = 0.35;
const FEEDBACK_NOTCH_RELEASE: f32 = 0.035;
const FEEDBACK_CANDIDATES_HZ: [f32; 8] = [
    900.0, 1200.0, 1600.0, 2200.0, 3000.0, 4200.0, 5600.0, 7200.0,
];
const FEEDBACK_MAX_NOTCHES: usize = 2;

#[repr(C)]
pub struct DspHandle {
    gains_db: [AtomicU32; EQ_BANDS.len()],
    cached_gains: [f32; EQ_BANDS.len()],
    biquads: [Biquad; EQ_BANDS.len()],
    noise_gate_enabled: AtomicBool,
    noise_strength: AtomicU32,
    noise_profile_mode: AtomicU32,
    capture_samples_remaining: i32,
    capture_age_samples: u64,
    spectral_nr: SpectralNoiseReducer,
    limiter_enabled: AtomicBool,
    perf_frames: i32,
    perf_sum_abs: f32,
    perf_level: AtomicU32,
    nr_avg_suppression_pct: AtomicU32,
    nr_fault_resets: AtomicU32,
    nr_capture_age_seconds: AtomicU32,
    xover_low_lp: [Biquad; 2],
    xover_low_hp: [Biquad; 2],
    xover_high_lp: [Biquad; 2],
    xover_high_hp: [Biquad; 2],
    comp_env_low: f32,
    comp_env_mid: f32,
    comp_env_high: f32,
    band_gain_low: AtomicU32,
    band_gain_mid: AtomicU32,
    band_gain_high: AtomicU32,
    feedback_broadband_env: f32,
    feedback_resonator_s1: [f32; FEEDBACK_CANDIDATES_HZ.len()],
    feedback_resonator_s2: [f32; FEEDBACK_CANDIDATES_HZ.len()],
    feedback_resonator_c1: [f32; FEEDBACK_CANDIDATES_HZ.len()],
    feedback_resonator_c2: [f32; FEEDBACK_CANDIDATES_HZ.len()],
    feedback_candidate_env: [f32; FEEDBACK_CANDIDATES_HZ.len()],
    feedback_notch_filters: [Biquad; FEEDBACK_MAX_NOTCHES],
    feedback_notch_freqs: [f32; FEEDBACK_MAX_NOTCHES],
    feedback_notch_mix: [f32; FEEDBACK_MAX_NOTCHES],
    feedback_notch_target_mix: [f32; FEEDBACK_MAX_NOTCHES],
    feedback_select_counter: usize,
    safe_mode: AtomicBool,
    master_gain: AtomicU32,
    agc_enabled: AtomicBool,
    agc_max_gain: AtomicU32,
    agc_env: f32,
    low_cut_hz: AtomicU32,
    high_cut_hz: AtomicU32,
    cached_low_cut_hz: f32,
    cached_high_cut_hz: f32,
    low_cut_filters: [Biquad; 2],
    high_cut_filters: [Biquad; 2],
    sample_rate: f32,
    coefficients_dirty: bool,
}

#[derive(Clone, Copy)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.b0 * input + self.z1;
        self.z1 = self.b1 * input - self.a1 * output + self.z2;
        self.z2 = self.b2 * input - self.a2 * output;
        output
    }
}

struct SpectralNoiseReducer {
    frame_size: usize,
    hop_size: usize,
    window: Vec<f32>,
    forward: Arc<dyn Fft<f32>>,
    inverse: Arc<dyn Fft<f32>>,
    spectrum: Vec<Complex32>,
    time: Vec<f32>,
    overlap: Vec<f32>,
    input: Vec<f32>,
    input_offset: usize,
    output: VecDeque<f32>,
    noise_psd: Vec<f32>,
    gain_smooth: Vec<f32>,
    initialized: bool,
    mix: f32,
    was_enabled: bool,
    dry: Vec<f32>,
    fault_count: u8,
    fault_resets: u32,
    avg_suppression_pct: f32,
}

#[derive(Clone, Copy)]
enum NoiseProfileMode {
    Comfort,
    Strong,
}

impl NoiseProfileMode {
    fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Strong,
            _ => Self::Comfort,
        }
    }
}

#[derive(Clone, Copy)]
struct NrProfileParams {
    effective_strength_max: f32,
    min_gain_base: f32,
    min_gain_range: f32,
    gain_attack: f32,
    gain_release: f32,
    mix_attack: f32,
    mix_release: f32,
    gain_step_limit: f32,
}

fn nr_profile_params(mode: NoiseProfileMode) -> NrProfileParams {
    match mode {
        NoiseProfileMode::Comfort => NrProfileParams {
            effective_strength_max: 0.72,
            min_gain_base: 0.34,
            min_gain_range: 0.42,
            gain_attack: 0.25,
            gain_release: 0.12,
            mix_attack: 0.015,
            mix_release: 0.05,
            gain_step_limit: SPECTRAL_NR_GAIN_STEP_LIMIT_COMFORT,
        },
        NoiseProfileMode::Strong => NrProfileParams {
            effective_strength_max: 0.92,
            min_gain_base: 0.16,
            min_gain_range: 0.30,
            gain_attack: 0.45,
            gain_release: 0.07,
            mix_attack: 0.03,
            mix_release: 0.035,
            gain_step_limit: SPECTRAL_NR_GAIN_STEP_LIMIT_STRONG,
        },
    }
}

impl SpectralNoiseReducer {
    fn new(frame_size: usize, hop_size: usize) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(frame_size);
        let inverse = planner.plan_fft_inverse(frame_size);

        let mut window = vec![0.0; frame_size];
        for (index, value) in window.iter_mut().enumerate() {
            let phase = core::f32::consts::PI * index as f32 / (frame_size as f32 - 1.0);
            *value = phase.sin().sqrt();
        }

        let output = VecDeque::with_capacity(frame_size * 2);

        Self {
            frame_size,
            hop_size,
            window,
            forward,
            inverse,
            spectrum: vec![Complex32::new(0.0, 0.0); frame_size],
            time: vec![0.0; frame_size],
            overlap: vec![0.0; frame_size - hop_size],
            input: Vec::with_capacity(frame_size * 4),
            input_offset: 0,
            output,
            noise_psd: vec![SPECTRAL_NR_INIT_NOISE; frame_size],
            gain_smooth: vec![1.0; frame_size],
            initialized: false,
            mix: 0.0,
            was_enabled: false,
            dry: Vec::with_capacity(frame_size * 2),
            fault_count: 0,
            fault_resets: 0,
            avg_suppression_pct: 0.0,
        }
    }

    fn reset_stream_state(&mut self) {
        self.overlap.fill(0.0);
        self.input.clear();
        self.input_offset = 0;
        self.output.clear();
        self.fault_count = 0;
    }

    fn begin_capture(&mut self) {
        self.noise_psd.fill(SPECTRAL_NR_INIT_NOISE);
        self.gain_smooth.fill(1.0);
        self.initialized = false;
        self.fault_count = 0;
    }

    fn process(
        &mut self,
        samples: &mut [f32],
        strength: f32,
        capture_samples_remaining: &mut i32,
        enabled: bool,
        sample_rate_hz: f32,
        mode: NoiseProfileMode,
    ) {
        if !enabled {
            if self.was_enabled {
                self.reset_stream_state();
            }
            self.was_enabled = false;
            self.mix = 0.0;
            self.avg_suppression_pct *= 0.9;
            return;
        }
        if !self.was_enabled {
            self.reset_stream_state();
            self.was_enabled = true;
        }
        let params = nr_profile_params(mode);

        self.dry.clear();
        self.dry.extend(
            samples
                .iter()
                .copied()
                .map(|v| if v.is_finite() { v } else { 0.0 }),
        );
        for (sample, dry) in samples.iter_mut().zip(self.dry.iter()) {
            *sample = *dry;
        }
        self.input.extend_from_slice(samples);

        while self.input.len().saturating_sub(self.input_offset) >= self.frame_size {
            let frame_start = self.input_offset;
            for index in 0..self.frame_size {
                let sample = self.input[frame_start + index] * self.window[index];
                self.spectrum[index] = Complex32::new(sample, 0.0);
            }

            self.forward.process(&mut self.spectrum);

            let capture_active = *capture_samples_remaining > 0;
            let mut frame_suppression = 0.0f32;
            let mut frame_bins = 0usize;
            for (bin, value) in self.spectrum.iter_mut().enumerate() {
                let mag2 = value.norm_sqr();
                if !self.initialized {
                    self.noise_psd[bin] = mag2.max(SPECTRAL_NR_INIT_NOISE);
                } else if capture_active {
                    self.noise_psd[bin] = SPECTRAL_NR_CAPTURE_ALPHA * self.noise_psd[bin]
                        + (1.0 - SPECTRAL_NR_CAPTURE_ALPHA) * mag2;
                } else if mag2 < self.noise_psd[bin] * 2.5 {
                    self.noise_psd[bin] = SPECTRAL_NR_ADAPT_ALPHA * self.noise_psd[bin]
                        + (1.0 - SPECTRAL_NR_ADAPT_ALPHA) * mag2;
                }

                let noise = self.noise_psd[bin].max(SPECTRAL_NR_EPS);
                let post_snr = ((mag2 - noise).max(0.0)) / noise;
                let wiener = post_snr / (1.0 + post_snr);
                let hz = (bin as f32 * sample_rate_hz / self.frame_size as f32).abs();
                let band_weight = nr_band_weight(hz);
                let effective_strength = (strength.clamp(0.0, 1.0) * band_weight).clamp(0.0, 1.0)
                    * params.effective_strength_max;
                let min_gain =
                    params.min_gain_base + (1.0 - effective_strength) * params.min_gain_range;
                let target_gain = (1.0 - effective_strength * (1.0 - wiener)).clamp(min_gain, 1.0);
                let previous_gain = self.gain_smooth[bin];
                let gain_alpha = if target_gain < previous_gain {
                    params.gain_attack
                } else {
                    params.gain_release
                };
                let proposed_gain = previous_gain + gain_alpha * (target_gain - previous_gain);
                let gain = proposed_gain.clamp(
                    previous_gain - params.gain_step_limit,
                    previous_gain + params.gain_step_limit,
                );
                self.gain_smooth[bin] = gain;
                *value *= gain;
                frame_suppression += (1.0 - gain).max(0.0);
                frame_bins += 1;
            }
            if frame_bins > 0 {
                let frame_pct = (frame_suppression / frame_bins as f32) * 100.0;
                self.avg_suppression_pct +=
                    SPECTRAL_NR_SUPPRESSION_SMOOTH * (frame_pct - self.avg_suppression_pct);
            }
            self.initialized = true;

            self.inverse.process(&mut self.spectrum);
            let scale = 1.0 / self.frame_size as f32;
            for index in 0..self.frame_size {
                let value = self.spectrum[index].re * scale * self.window[index];
                self.time[index] = if value.is_finite() { value } else { 0.0 };
            }

            for index in 0..self.overlap.len() {
                self.time[index] += self.overlap[index];
            }
            for index in 0..self.hop_size {
                self.output.push_back(self.time[index]);
            }
            self.overlap
                .copy_from_slice(&self.time[self.hop_size..self.frame_size]);

            self.input_offset += self.hop_size;
            if *capture_samples_remaining > 0 {
                *capture_samples_remaining =
                    (*capture_samples_remaining - self.hop_size as i32).max(0);
            }
        }

        if self.input_offset > self.frame_size * 2 {
            self.input.drain(0..self.input_offset);
            self.input_offset = 0;
        }

        let target_mix = strength.clamp(0.0, 1.0) * params.effective_strength_max;
        let mut in_level = 0.0f32;
        let mut out_level = 0.0f32;
        for (index, sample) in samples.iter_mut().enumerate() {
            let alpha = if target_mix > self.mix {
                params.mix_attack
            } else {
                params.mix_release
            };
            self.mix += alpha * (target_mix - self.mix);
            if let Some(filtered) = self.output.pop_front() {
                *sample = self.dry[index] + self.mix * (filtered - self.dry[index]);
            }
            if !sample.is_finite() {
                *sample = self.dry[index];
            }
            in_level += self.dry[index].abs();
            out_level += sample.abs();
        }

        let frames = samples.len().max(1) as f32;
        let in_avg = in_level / frames;
        let out_avg = out_level / frames;
        if in_avg > SPECTRAL_NR_FAULT_IN_LEVEL && out_avg < in_avg * SPECTRAL_NR_FAULT_OUT_RATIO {
            self.fault_count = self.fault_count.saturating_add(1);
        } else {
            self.fault_count = self.fault_count.saturating_sub(1);
        }
        if self.fault_count >= SPECTRAL_NR_FAULT_LIMIT {
            for (sample, dry) in samples.iter_mut().zip(self.dry.iter()) {
                *sample = *dry;
            }
            self.reset_stream_state();
            self.mix = 0.0;
            self.fault_resets = self.fault_resets.saturating_add(1);
        }
    }

    fn fault_resets(&self) -> u32 {
        self.fault_resets
    }

    fn avg_suppression_pct(&self) -> f32 {
        self.avg_suppression_pct.max(0.0)
    }
}

impl Default for Biquad {
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
        }
    }
}

impl DspHandle {
    fn new() -> Self {
        Self {
            gains_db: std::array::from_fn(|_| AtomicU32::new(0.0f32.to_bits())),
            cached_gains: [0.0; EQ_BANDS.len()],
            biquads: [Biquad::default(); EQ_BANDS.len()],
            noise_gate_enabled: AtomicBool::new(false),
            noise_strength: AtomicU32::new(0.6f32.to_bits()),
            noise_profile_mode: AtomicU32::new(0),
            capture_samples_remaining: 0,
            capture_age_samples: 0,
            spectral_nr: SpectralNoiseReducer::new(SPECTRAL_NR_FRAME_SIZE, SPECTRAL_NR_HOP_SIZE),
            limiter_enabled: AtomicBool::new(true),
            perf_frames: 0,
            perf_sum_abs: 0.0,
            perf_level: AtomicU32::new(0.0f32.to_bits()),
            nr_avg_suppression_pct: AtomicU32::new(0.0f32.to_bits()),
            nr_fault_resets: AtomicU32::new(0),
            nr_capture_age_seconds: AtomicU32::new(0.0f32.to_bits()),
            xover_low_lp: [Biquad::default(); 2],
            xover_low_hp: [Biquad::default(); 2],
            xover_high_lp: [Biquad::default(); 2],
            xover_high_hp: [Biquad::default(); 2],
            comp_env_low: 0.0,
            comp_env_mid: 0.0,
            comp_env_high: 0.0,
            band_gain_low: AtomicU32::new(1.0f32.to_bits()),
            band_gain_mid: AtomicU32::new(1.0f32.to_bits()),
            band_gain_high: AtomicU32::new(1.0f32.to_bits()),
            feedback_broadband_env: 0.0,
            feedback_resonator_s1: [0.0; FEEDBACK_CANDIDATES_HZ.len()],
            feedback_resonator_s2: [0.0; FEEDBACK_CANDIDATES_HZ.len()],
            feedback_resonator_c1: [0.0; FEEDBACK_CANDIDATES_HZ.len()],
            feedback_resonator_c2: [0.0; FEEDBACK_CANDIDATES_HZ.len()],
            feedback_candidate_env: [0.0; FEEDBACK_CANDIDATES_HZ.len()],
            feedback_notch_filters: [Biquad::default(); FEEDBACK_MAX_NOTCHES],
            feedback_notch_freqs: [0.0; FEEDBACK_MAX_NOTCHES],
            feedback_notch_mix: [0.0; FEEDBACK_MAX_NOTCHES],
            feedback_notch_target_mix: [0.0; FEEDBACK_MAX_NOTCHES],
            feedback_select_counter: 0,
            safe_mode: AtomicBool::new(true),
            master_gain: AtomicU32::new(1.25f32.to_bits()),
            agc_enabled: AtomicBool::new(true),
            agc_max_gain: AtomicU32::new(1.0f32.to_bits()),
            agc_env: 0.0,
            low_cut_hz: AtomicU32::new(DEFAULT_LOW_CUT_HZ.to_bits()),
            high_cut_hz: AtomicU32::new(DEFAULT_HIGH_CUT_HZ.to_bits()),
            cached_low_cut_hz: DEFAULT_LOW_CUT_HZ,
            cached_high_cut_hz: DEFAULT_HIGH_CUT_HZ,
            low_cut_filters: [Biquad::default(); 2],
            high_cut_filters: [Biquad::default(); 2],
            sample_rate: 0.0,
            coefficients_dirty: true,
        }
    }

    fn set_gain_db(&self, index: usize, value: f32) {
        if index >= self.gains_db.len() {
            return;
        }
        self.gains_db[index].store(value.to_bits(), Ordering::Relaxed);
    }

    fn set_noise_gate(&self, enabled: bool) {
        self.noise_gate_enabled.store(enabled, Ordering::Relaxed);
    }

    fn set_noise_strength(&self, strength: f32) {
        let clamped = strength.clamp(0.0, 1.0);
        self.noise_strength
            .store(clamped.to_bits(), Ordering::Relaxed);
    }

    fn set_noise_profile_mode(&self, mode: i32) {
        self.noise_profile_mode
            .store(mode.clamp(0, 1) as u32, Ordering::Relaxed);
    }

    fn set_limiter_enabled(&self, enabled: bool) {
        self.limiter_enabled.store(enabled, Ordering::Relaxed);
    }

    fn set_band_gains(&self, low: f32, mid: f32, high: f32) {
        self.band_gain_low
            .store(low.clamp(0.5, MAX_BAND_GAIN).to_bits(), Ordering::Relaxed);
        self.band_gain_mid
            .store(mid.clamp(0.5, MAX_BAND_GAIN).to_bits(), Ordering::Relaxed);
        self.band_gain_high
            .store(high.clamp(0.5, MAX_BAND_GAIN).to_bits(), Ordering::Relaxed);
    }

    fn set_safe_mode(&self, enabled: bool) {
        self.safe_mode.store(enabled, Ordering::Relaxed);
    }

    fn set_master_gain(&self, gain: f32) {
        let clamped = gain.clamp(MASTER_GAIN_MIN, MASTER_GAIN_MAX);
        self.master_gain.store(clamped.to_bits(), Ordering::Relaxed);
    }

    fn set_agc_enabled(&self, enabled: bool) {
        self.agc_enabled.store(enabled, Ordering::Relaxed);
    }

    fn set_agc_max_gain(&self, gain: f32) {
        let clamped = gain.clamp(AGC_MAX_GAIN_MIN, AGC_MAX_GAIN_MAX);
        self.agc_max_gain
            .store(clamped.to_bits(), Ordering::Relaxed);
    }

    fn set_low_cut_hz(&self, hz: f32) {
        let clamped = hz.clamp(LOW_CUT_MIN_HZ, LOW_CUT_MAX_HZ);
        self.low_cut_hz.store(clamped.to_bits(), Ordering::Relaxed);
    }

    fn set_high_cut_hz(&self, hz: f32) {
        let clamped = hz.clamp(HIGH_CUT_MIN_HZ, HIGH_CUT_MAX_HZ);
        self.high_cut_hz.store(clamped.to_bits(), Ordering::Relaxed);
    }

    fn capture_noise_profile(&mut self) {
        if self.sample_rate > 0.0 {
            self.capture_samples_remaining = (self.sample_rate * NOISE_CAPTURE_SEC) as i32;
            self.spectral_nr.begin_capture();
            self.capture_age_samples = 0;
            self.nr_capture_age_seconds
                .store(0.0f32.to_bits(), Ordering::Relaxed);
        } else {
            self.capture_samples_remaining = -1;
        }
    }

    fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate <= 0.0 {
            return;
        }
        if (sample_rate - self.sample_rate).abs() > 1.0 {
            self.sample_rate = sample_rate;
            self.coefficients_dirty = true;
            self.spectral_nr.reset_stream_state();
            self.update_crossovers();
            self.update_feedback_coefficients();
        }
    }

    fn process_buffer(&mut self, samples: &mut [f32]) {
        let gains_changed = self.refresh_gains();
        let cuts_changed = self.refresh_cuts();
        if self.coefficients_dirty || gains_changed || cuts_changed {
            self.update_coefficients();
        }

        let noise_gate = self.noise_gate_enabled.load(Ordering::Relaxed);
        let strength = f32::from_bits(self.noise_strength.load(Ordering::Relaxed));
        let mode =
            NoiseProfileMode::from_i32(self.noise_profile_mode.load(Ordering::Relaxed) as i32);
        let was_capturing = self.capture_samples_remaining > 0;
        self.spectral_nr.process(
            samples,
            strength,
            &mut self.capture_samples_remaining,
            noise_gate,
            self.sample_rate,
            mode,
        );
        let now_capturing = self.capture_samples_remaining > 0;
        if now_capturing {
            self.capture_age_samples = 0;
        } else {
            if was_capturing {
                self.capture_age_samples = 0;
            }
            self.capture_age_samples = self
                .capture_age_samples
                .saturating_add(samples.len() as u64);
        }
        let capture_age_seconds = if self.sample_rate > 0.0 {
            self.capture_age_samples as f32 / self.sample_rate
        } else {
            0.0
        };
        self.nr_capture_age_seconds
            .store(capture_age_seconds.to_bits(), Ordering::Relaxed);
        self.nr_avg_suppression_pct.store(
            self.spectral_nr.avg_suppression_pct().to_bits(),
            Ordering::Relaxed,
        );
        self.nr_fault_resets
            .store(self.spectral_nr.fault_resets(), Ordering::Relaxed);

        for sample in samples.iter_mut() {
            let mut value = *sample;

            let agc_enabled = self.agc_enabled.load(Ordering::Relaxed);
            let agc_max_gain = f32::from_bits(self.agc_max_gain.load(Ordering::Relaxed));
            if agc_enabled {
                let level = value.abs();
                let alpha = if level > self.agc_env {
                    AGC_ATTACK
                } else {
                    AGC_RELEASE
                };
                self.agc_env += alpha * (level - self.agc_env);
                // Avoid boosting near-silence/hiss, otherwise normalize around target level.
                let desired_gain = if self.agc_env < AGC_SILENCE_LEVEL {
                    1.0
                } else {
                    (AGC_TARGET / (self.agc_env + 1.0e-6)).clamp(AGC_GAIN_MIN, agc_max_gain)
                };
                value *= desired_gain;
            }

            value = self.process_multiband(value);
            value = self.apply_feedback_notches(value);
            value = process_chain(&mut self.low_cut_filters, value);
            value = process_chain(&mut self.high_cut_filters, value);

            for biquad in &mut self.biquads {
                value = biquad.process(value);
            }

            let master_gain = f32::from_bits(self.master_gain.load(Ordering::Relaxed));
            value *= master_gain;

            if self.limiter_enabled.load(Ordering::Relaxed) {
                *sample = apply_limiter(value);
            } else {
                *sample = value;
            }

            self.perf_sum_abs += sample.abs();
        }

        self.perf_frames += samples.len() as i32;
        if self.sample_rate > 0.0 {
            let window_samples = (self.sample_rate * PERF_WINDOW_SEC) as i32;
            if self.perf_frames >= window_samples {
                let avg = self.perf_sum_abs / self.perf_frames.max(1) as f32;
                self.perf_frames = 0;
                self.perf_sum_abs = 0.0;
                self.set_perf_level(avg);
            }
        }
    }

    fn refresh_gains(&mut self) -> bool {
        let mut changed = false;
        for (index, cached) in self.cached_gains.iter_mut().enumerate() {
            let value = f32::from_bits(self.gains_db[index].load(Ordering::Relaxed));
            if (value - *cached).abs() > 0.001 {
                *cached = value;
                changed = true;
            }
        }
        changed
    }

    fn refresh_cuts(&mut self) -> bool {
        let low = f32::from_bits(self.low_cut_hz.load(Ordering::Relaxed));
        let high = f32::from_bits(self.high_cut_hz.load(Ordering::Relaxed));
        let mut changed = false;
        if (low - self.cached_low_cut_hz).abs() > 0.5 {
            self.cached_low_cut_hz = low;
            changed = true;
        }
        if (high - self.cached_high_cut_hz).abs() > 0.5 {
            self.cached_high_cut_hz = high;
            changed = true;
        }
        changed
    }

    fn update_coefficients(&mut self) {
        if self.sample_rate <= 0.0 {
            return;
        }

        for ((biquad, gain_db), frequency) in self
            .biquads
            .iter_mut()
            .zip(self.cached_gains.iter())
            .zip(EQ_BANDS.iter())
        {
            let a = 10.0_f32.powf(*gain_db / 40.0);
            let omega = TWO_PI * frequency / self.sample_rate;
            let sn = omega.sin();
            let cs = omega.cos();
            let alpha = sn / (2.0 * BIQUAD_Q);

            let b0 = 1.0 + alpha * a;
            let b1 = -2.0 * cs;
            let b2 = 1.0 - alpha * a;
            let a0 = 1.0 + alpha / a;
            let a1 = -2.0 * cs;
            let a2 = 1.0 - alpha / a;

            biquad.b0 = b0 / a0;
            biquad.b1 = b1 / a0;
            biquad.b2 = b2 / a0;
            biquad.a1 = a1 / a0;
            biquad.a2 = a2 / a0;
        }
        design_highpass(
            &mut self.low_cut_filters,
            self.sample_rate,
            self.cached_low_cut_hz,
        );
        design_lowpass(
            &mut self.high_cut_filters,
            self.sample_rate,
            self.cached_high_cut_hz,
        );

        self.coefficients_dirty = false;
    }

    fn update_crossovers(&mut self) {
        if self.sample_rate <= 0.0 {
            return;
        }
        design_lowpass(&mut self.xover_low_lp, self.sample_rate, CROSSOVER_LOW_HZ);
        design_highpass(&mut self.xover_low_hp, self.sample_rate, CROSSOVER_LOW_HZ);
        design_lowpass(&mut self.xover_high_lp, self.sample_rate, CROSSOVER_HIGH_HZ);
        design_highpass(&mut self.xover_high_hp, self.sample_rate, CROSSOVER_HIGH_HZ);
    }

    fn update_feedback_coefficients(&mut self) {
        if self.sample_rate <= 0.0 {
            return;
        }
        for (index, frequency_hz) in FEEDBACK_CANDIDATES_HZ.iter().enumerate() {
            let omega = TWO_PI * frequency_hz / self.sample_rate;
            let r = FEEDBACK_RESONATOR_DAMPING;
            self.feedback_resonator_c1[index] = 2.0 * r * omega.cos();
            self.feedback_resonator_c2[index] = -(r * r);
            self.feedback_resonator_s1[index] = 0.0;
            self.feedback_resonator_s2[index] = 0.0;
            self.feedback_candidate_env[index] = 0.0;
        }
        self.feedback_broadband_env = 0.0;
        self.feedback_select_counter = 0;
        self.feedback_notch_mix = [0.0; FEEDBACK_MAX_NOTCHES];
        self.feedback_notch_target_mix = [0.0; FEEDBACK_MAX_NOTCHES];
        self.feedback_notch_freqs = [0.0; FEEDBACK_MAX_NOTCHES];
        self.feedback_notch_filters = [Biquad::default(); FEEDBACK_MAX_NOTCHES];
    }

    fn apply_feedback_notches(&mut self, input: f32) -> f32 {
        let level = input.abs();
        let alpha = if level > self.feedback_broadband_env {
            FEEDBACK_ENV_ATTACK
        } else {
            FEEDBACK_ENV_RELEASE
        };
        self.feedback_broadband_env += alpha * (level - self.feedback_broadband_env);

        for index in 0..FEEDBACK_CANDIDATES_HZ.len() {
            let s0 = input
                + self.feedback_resonator_c1[index] * self.feedback_resonator_s1[index]
                + self.feedback_resonator_c2[index] * self.feedback_resonator_s2[index];
            self.feedback_resonator_s2[index] = self.feedback_resonator_s1[index];
            self.feedback_resonator_s1[index] = s0;
            let power = (s0 * s0).abs();
            self.feedback_candidate_env[index] +=
                FEEDBACK_ENV_ATTACK * (power - self.feedback_candidate_env[index]);
        }

        self.feedback_select_counter = self.feedback_select_counter.wrapping_add(1);
        if self.feedback_select_counter >= FEEDBACK_SELECT_INTERVAL {
            self.feedback_select_counter = 0;
            self.retarget_feedback_notches();
        }

        for index in 0..FEEDBACK_MAX_NOTCHES {
            let target = self.feedback_notch_target_mix[index];
            let current = &mut self.feedback_notch_mix[index];
            let alpha = if target > *current {
                FEEDBACK_NOTCH_ATTACK
            } else {
                FEEDBACK_NOTCH_RELEASE
            };
            *current += alpha * (target - *current);
        }

        let mut value = input;
        for index in 0..FEEDBACK_MAX_NOTCHES {
            let mix = self.feedback_notch_mix[index];
            if mix <= 0.001 {
                continue;
            }
            let notched = self.feedback_notch_filters[index].process(value);
            value = value + mix * (notched - value);
        }
        value
    }

    fn retarget_feedback_notches(&mut self) {
        let base = (self.feedback_broadband_env * self.feedback_broadband_env).max(1.0e-7);
        let mut scored = [(0.0f32, 0.0f32); FEEDBACK_CANDIDATES_HZ.len()];
        for (index, freq) in FEEDBACK_CANDIDATES_HZ.iter().enumerate() {
            let ratio = self.feedback_candidate_env[index] / base;
            scored[index] = (ratio, *freq);
        }

        self.feedback_notch_target_mix = [0.0; FEEDBACK_MAX_NOTCHES];
        for slot in 0..FEEDBACK_MAX_NOTCHES {
            let mut best_index = None;
            let mut best_ratio = f32::NEG_INFINITY;
            for (index, (ratio, _)) in scored.iter().enumerate() {
                if *ratio > best_ratio {
                    best_ratio = *ratio;
                    best_index = Some(index);
                }
            }
            let Some(index) = best_index else {
                continue;
            };
            let (ratio, freq) = scored[index];
            scored[index].0 = f32::NEG_INFINITY;
            if ratio < FEEDBACK_RATIO_THRESHOLD {
                continue;
            }
            let norm =
                ((ratio - FEEDBACK_RATIO_THRESHOLD) / FEEDBACK_RATIO_THRESHOLD).clamp(0.0, 1.0);
            let target_mix = norm * FEEDBACK_MAX_NOTCH_MIX;
            self.feedback_notch_target_mix[slot] = target_mix;
            if (self.feedback_notch_freqs[slot] - freq).abs() > 10.0 {
                self.feedback_notch_freqs[slot] = freq;
                design_notch(
                    &mut self.feedback_notch_filters[slot],
                    self.sample_rate,
                    freq,
                    FEEDBACK_NOTCH_Q,
                );
            }
        }
    }

    fn process_multiband(&mut self, input: f32) -> f32 {
        let low = process_chain(&mut self.xover_low_lp, input);
        let high = process_chain(&mut self.xover_high_hp, input);
        let mid = process_chain(
            &mut self.xover_high_lp,
            process_chain(&mut self.xover_low_hp, input),
        );

        let mut low = apply_compressor(low, &mut self.comp_env_low);
        let mut mid = apply_compressor(mid, &mut self.comp_env_mid);
        let mut high = apply_compressor(high, &mut self.comp_env_high);

        let mut gain_low = f32::from_bits(self.band_gain_low.load(Ordering::Relaxed));
        let mut gain_mid = f32::from_bits(self.band_gain_mid.load(Ordering::Relaxed));
        let mut gain_high = f32::from_bits(self.band_gain_high.load(Ordering::Relaxed));

        if self.safe_mode.load(Ordering::Relaxed) {
            gain_low *= SAFE_MODE_GAIN;
            gain_mid *= SAFE_MODE_GAIN;
            gain_high *= SAFE_MODE_GAIN;
        }

        low *= gain_low;
        mid *= gain_mid;
        high *= gain_high;

        let mut mixed = low + mid + high;
        if self.safe_mode.load(Ordering::Relaxed) {
            mixed = mixed.clamp(-0.8, 0.8);
        }
        mixed
    }

    fn set_perf_level(&self, level: f32) {
        self.perf_level.store(level.to_bits(), Ordering::Relaxed);
    }
}

fn apply_limiter(sample: f32) -> f32 {
    let abs = sample.abs();
    if abs <= LIMITER_THRESHOLD {
        return sample;
    }

    let excess = abs - LIMITER_THRESHOLD;
    let compressed = LIMITER_THRESHOLD + (excess / (1.0 + excess));
    sample.signum() * compressed.min(1.0)
}

fn nr_band_weight(frequency_hz: f32) -> f32 {
    let hz = frequency_hz.max(0.0);
    if hz < 180.0 {
        1.20
    } else if hz < 500.0 {
        1.20 - (hz - 180.0) * (0.20 / 320.0)
    } else if hz <= 3000.0 {
        0.72
    } else if hz < 4000.0 {
        0.72 + (hz - 3000.0) * (0.33 / 1000.0)
    } else if hz < 9000.0 {
        1.05 + (hz - 4000.0) * (0.28 / 5000.0)
    } else {
        1.20
    }
}

fn process_chain(chain: &mut [Biquad; 2], input: f32) -> f32 {
    let mut value = input;
    for biquad in chain.iter_mut() {
        value = biquad.process(value);
    }
    value
}

fn apply_compressor(sample: f32, env: &mut f32) -> f32 {
    let level = sample.abs();
    let alpha = if level > *env {
        COMP_ATTACK
    } else {
        COMP_RELEASE
    };
    *env += alpha * (level - *env);

    let level_db = 20.0 * (*env + 1.0e-6).log10();
    let mut gain_db = 0.0;
    if level_db > COMP_THRESHOLD_DB {
        let over_db = level_db - COMP_THRESHOLD_DB;
        let compressed_db = over_db / COMP_RATIO;
        gain_db = (COMP_THRESHOLD_DB + compressed_db) - level_db;
    }

    let gain = 10.0_f32.powf(gain_db / 20.0);
    sample * gain
}

fn design_lowpass(chain: &mut [Biquad; 2], sample_rate: f32, cutoff_hz: f32) {
    let omega = TWO_PI * cutoff_hz / sample_rate;
    let sn = omega.sin();
    let cs = omega.cos();
    let alpha = sn / (2.0 * std::f32::consts::FRAC_1_SQRT_2);

    let b0 = (1.0 - cs) * 0.5;
    let b1 = 1.0 - cs;
    let b2 = (1.0 - cs) * 0.5;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cs;
    let a2 = 1.0 - alpha;

    for biquad in chain.iter_mut() {
        biquad.b0 = b0 / a0;
        biquad.b1 = b1 / a0;
        biquad.b2 = b2 / a0;
        biquad.a1 = a1 / a0;
        biquad.a2 = a2 / a0;
        biquad.reset();
    }
}

fn design_highpass(chain: &mut [Biquad; 2], sample_rate: f32, cutoff_hz: f32) {
    let omega = TWO_PI * cutoff_hz / sample_rate;
    let sn = omega.sin();
    let cs = omega.cos();
    let alpha = sn / (2.0 * std::f32::consts::FRAC_1_SQRT_2);

    let b0 = (1.0 + cs) * 0.5;
    let b1 = -(1.0 + cs);
    let b2 = (1.0 + cs) * 0.5;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cs;
    let a2 = 1.0 - alpha;

    for biquad in chain.iter_mut() {
        biquad.b0 = b0 / a0;
        biquad.b1 = b1 / a0;
        biquad.b2 = b2 / a0;
        biquad.a1 = a1 / a0;
        biquad.a2 = a2 / a0;
        biquad.reset();
    }
}

fn design_notch(filter: &mut Biquad, sample_rate: f32, frequency_hz: f32, q: f32) {
    if sample_rate <= 0.0 || frequency_hz <= 0.0 {
        *filter = Biquad::default();
        return;
    }
    let omega = TWO_PI * frequency_hz / sample_rate;
    let sn = omega.sin();
    let cs = omega.cos();
    let alpha = sn / (2.0 * q.max(0.1));

    let b0 = 1.0;
    let b1 = -2.0 * cs;
    let b2 = 1.0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cs;
    let a2 = 1.0 - alpha;

    filter.b0 = b0 / a0;
    filter.b1 = b1 / a0;
    filter.b2 = b2 / a0;
    filter.a1 = a1 / a0;
    filter.a2 = a2 / a0;
    filter.reset();
}

#[no_mangle]
pub extern "C" fn hear_buds_dsp_create() -> *mut DspHandle {
    Box::into_raw(Box::new(DspHandle::new()))
}

/// # Safety
/// Caller must pass a pointer previously returned by `hear_buds_dsp_create`.
#[no_mangle]
pub unsafe extern "C" fn hear_buds_dsp_destroy(handle: *mut DspHandle) {
    if handle.is_null() {
        return;
    }
    drop(Box::from_raw(handle));
}

/// # Safety
/// Caller must pass a valid `DspHandle` pointer.
#[no_mangle]
pub unsafe extern "C" fn hear_buds_dsp_set_eq_band(
    handle: *mut DspHandle,
    index: i32,
    value_db: f32,
) {
    if handle.is_null() || index < 0 {
        return;
    }
    (*handle).set_gain_db(index as usize, value_db);
}

/// # Safety
/// Caller must pass a valid `DspHandle` pointer.
#[no_mangle]
pub unsafe extern "C" fn hear_buds_dsp_set_noise_cancel(handle: *mut DspHandle, enabled: bool) {
    if handle.is_null() {
        return;
    }
    (*handle).set_noise_gate(enabled);
}

/// # Safety
/// Caller must pass a valid `DspHandle` pointer.
#[no_mangle]
pub unsafe extern "C" fn hear_buds_dsp_set_noise_strength(handle: *mut DspHandle, strength: f32) {
    if handle.is_null() {
        return;
    }
    (*handle).set_noise_strength(strength);
}

/// # Safety
/// Caller must pass a valid `DspHandle` pointer.
#[no_mangle]
pub unsafe extern "C" fn hear_buds_dsp_set_noise_profile_mode(handle: *mut DspHandle, mode: i32) {
    if handle.is_null() {
        return;
    }
    (*handle).set_noise_profile_mode(mode);
}

/// # Safety
/// Caller must pass a valid `DspHandle` pointer.
#[no_mangle]
pub unsafe extern "C" fn hear_buds_dsp_set_limiter_enabled(handle: *mut DspHandle, enabled: bool) {
    if handle.is_null() {
        return;
    }
    (*handle).set_limiter_enabled(enabled);
}

/// # Safety
/// Caller must pass a valid `DspHandle` pointer.
#[no_mangle]
pub unsafe extern "C" fn hear_buds_dsp_set_band_gains(
    handle: *mut DspHandle,
    low: f32,
    mid: f32,
    high: f32,
) {
    if handle.is_null() {
        return;
    }
    (*handle).set_band_gains(low, mid, high);
}

/// # Safety
/// Caller must pass a valid `DspHandle` pointer.
#[no_mangle]
pub unsafe extern "C" fn hear_buds_dsp_set_safe_mode(handle: *mut DspHandle, enabled: bool) {
    if handle.is_null() {
        return;
    }
    (*handle).set_safe_mode(enabled);
}

/// # Safety
/// Caller must pass a valid `DspHandle` pointer.
#[no_mangle]
pub unsafe extern "C" fn hear_buds_dsp_set_master_gain(handle: *mut DspHandle, gain: f32) {
    if handle.is_null() {
        return;
    }
    (*handle).set_master_gain(gain);
}

/// # Safety
/// Caller must pass a valid `DspHandle` pointer.
#[no_mangle]
pub unsafe extern "C" fn hear_buds_dsp_set_agc_enabled(handle: *mut DspHandle, enabled: bool) {
    if handle.is_null() {
        return;
    }
    (*handle).set_agc_enabled(enabled);
}

/// # Safety
/// Caller must pass a valid `DspHandle` pointer.
#[no_mangle]
pub unsafe extern "C" fn hear_buds_dsp_set_agc_max_gain(handle: *mut DspHandle, gain: f32) {
    if handle.is_null() {
        return;
    }
    (*handle).set_agc_max_gain(gain);
}

/// # Safety
/// Caller must pass a valid `DspHandle` pointer.
#[no_mangle]
pub unsafe extern "C" fn hear_buds_dsp_set_low_cut_hz(handle: *mut DspHandle, hz: f32) {
    if handle.is_null() {
        return;
    }
    (*handle).set_low_cut_hz(hz);
}

/// # Safety
/// Caller must pass a valid `DspHandle` pointer.
#[no_mangle]
pub unsafe extern "C" fn hear_buds_dsp_set_high_cut_hz(handle: *mut DspHandle, hz: f32) {
    if handle.is_null() {
        return;
    }
    (*handle).set_high_cut_hz(hz);
}

/// # Safety
/// Caller must pass a valid `DspHandle` pointer.
#[no_mangle]
pub unsafe extern "C" fn hear_buds_dsp_capture_noise_profile(handle: *mut DspHandle) {
    if handle.is_null() {
        return;
    }
    (*handle).capture_noise_profile();
}

/// # Safety
/// Caller must pass a valid `DspHandle` pointer.
#[no_mangle]
pub unsafe extern "C" fn hear_buds_dsp_get_perf_level(handle: *mut DspHandle) -> f32 {
    if handle.is_null() {
        return 0.0;
    }
    f32::from_bits((*handle).perf_level.load(Ordering::Relaxed))
}

/// # Safety
/// Caller must pass a valid `DspHandle` pointer.
#[no_mangle]
pub unsafe extern "C" fn hear_buds_dsp_get_nr_avg_suppression_pct(handle: *mut DspHandle) -> f32 {
    if handle.is_null() {
        return 0.0;
    }
    f32::from_bits((*handle).nr_avg_suppression_pct.load(Ordering::Relaxed))
}

/// # Safety
/// Caller must pass a valid `DspHandle` pointer.
#[no_mangle]
pub unsafe extern "C" fn hear_buds_dsp_get_nr_fault_resets(handle: *mut DspHandle) -> u32 {
    if handle.is_null() {
        return 0;
    }
    (*handle).nr_fault_resets.load(Ordering::Relaxed)
}

/// # Safety
/// Caller must pass a valid `DspHandle` pointer.
#[no_mangle]
pub unsafe extern "C" fn hear_buds_dsp_get_nr_capture_age_seconds(handle: *mut DspHandle) -> f32 {
    if handle.is_null() {
        return 0.0;
    }
    f32::from_bits((*handle).nr_capture_age_seconds.load(Ordering::Relaxed))
}

/// # Safety
/// Caller must pass a valid `DspHandle` pointer and a writable buffer.
#[no_mangle]
pub unsafe extern "C" fn hear_buds_dsp_process(
    handle: *mut DspHandle,
    samples: *mut f32,
    frames: i32,
    sample_rate: f32,
) {
    if handle.is_null() || samples.is_null() || frames <= 0 {
        return;
    }

    let buffer = std::slice::from_raw_parts_mut(samples, frames as usize);
    let dsp = &mut *handle;
    dsp.set_sample_rate(sample_rate);
    dsp.process_buffer(buffer);
}
