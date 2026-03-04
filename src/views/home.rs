use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::dsp::{DefaultEngine, DspCommand, DspEngine, EngineState, SweepStep};
use crate::permissions::{
    check_microphone_permission, request_microphone_permission, PermissionState,
};
use crate::state::{
    combine_eq_layers, load_settings, save_settings, AppSettings, SweepResult, UserProfile,
};
use crate::sweep::{derive_band_gains, derive_eq_from_sweep};

const NOISE_CAPTURE_UI_MS: u64 = 2200;

#[derive(Clone, PartialEq)]
struct BeepSweep {
    running: bool,
    step: usize,
    completed: bool,
    bands: Vec<CalibrationBand>,
}

#[derive(Clone, PartialEq)]
struct CalibrationBand {
    frequency_hz: u32,
    current_db: f32,
    step_db: f32,
    reversals: u8,
    last_heard: Option<bool>,
}

impl Default for BeepSweep {
    fn default() -> Self {
        let frequencies = vec![1000, 2000, 4000, 8000, 500, 250, 125];
        let bands = frequencies
            .into_iter()
            .map(|frequency_hz| CalibrationBand {
                frequency_hz,
                current_db: -30.0,
                step_db: 5.0,
                reversals: 0,
                last_heard: None,
            })
            .collect();

        Self {
            running: false,
            step: 0,
            completed: false,
            bands,
        }
    }
}

/// The main screen for HearBuds.
#[component]
pub fn Home() -> Element {
    let mut settings = use_signal(AppSettings::default);
    let mut eq_editable = use_signal(|| false);
    let mut sweep = use_signal(BeepSweep::default);
    let mut mic_permission = use_signal(|| PermissionState::Denied);
    let mut engine = use_signal(|| Rc::new(RefCell::new(DefaultEngine::new())));
    let mut engine_state = use_signal(|| EngineState::Stopped);
    let mut noise_capture = use_signal(|| false);
    let callback_ms = use_signal_sync(|| 0.0f32);
    let input_peak = use_signal_sync(|| 0.0f32);
    let underruns = use_signal_sync(|| 0u64);
    let trimmed_samples = use_signal_sync(|| 0u64);
    let clipped_samples = use_signal_sync(|| 0u64);
    let nr_avg_suppression_pct = use_signal_sync(|| 0.0f32);
    let nr_fault_resets = use_signal_sync(|| 0u64);
    let nr_capture_age_seconds = use_signal_sync(|| 0.0f32);
    let route_status = use_signal_sync(|| String::from("Input: Unknown | Output: Unknown"));
    #[cfg(target_os = "android")]
    let perf_level = use_signal_sync(|| 0.0f32);
    let mut android_backend = use_signal(|| 0i32);
    let mut android_burst = use_signal(|| 192u32);
    let mut android_input_device_id = use_signal(|| 0i32);
    let mut android_output_device_id = use_signal(|| 0i32);
    let android_input_devices = use_signal(|| vec![(0, "Automatic".to_string())]);
    let android_output_devices = use_signal(|| vec![(0, "Automatic".to_string())]);
    let mut missed_responses = use_signal(|| 0u8);
    let mut show_advanced = use_signal(|| false);
    let mut calibration_mode = use_signal(|| false);
    let mut profile_name = use_signal(|| String::from("Profile 1"));
    let mut selected_profile = use_signal(|| 0usize);
    let mut pending_overwrite_index = use_signal(|| None::<usize>);
    let mut profile_feedback = use_signal(String::new);
    let mut profile_feedback_class = use_signal(|| String::from("success-text"));

    #[cfg(target_os = "android")]
    {
        use crate::dsp::AndroidEngine;
        AndroidEngine::attach_perf_signal(perf_level);
        AndroidEngine::attach_route_signal(route_status);
    }

    use_future(move || {
        let mut callback_ms = callback_ms;
        let mut engine = engine;
        async move {
            loop {
                let value = engine.with_mut(|engine| engine.borrow().callback_ms());
                callback_ms.set(value);
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    });

    use_future(move || {
        let mut input_peak = input_peak;
        let mut engine = engine;
        async move {
            loop {
                let value = engine.with_mut(|engine| engine.borrow().input_peak());
                input_peak.set(value);
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    });

    use_future(move || {
        let mut engine_state = engine_state;
        let mut engine = engine;
        async move {
            loop {
                let state = engine.with_mut(|engine| engine.borrow().state());
                engine_state.set(state);
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    });

    use_future(move || {
        let mut underruns = underruns;
        let mut trimmed_samples = trimmed_samples;
        let mut clipped_samples = clipped_samples;
        let mut nr_avg_suppression_pct = nr_avg_suppression_pct;
        let mut nr_fault_resets = nr_fault_resets;
        let mut nr_capture_age_seconds = nr_capture_age_seconds;
        let mut engine = engine;
        async move {
            loop {
                let (u, t, c, nr_pct, nr_resets, nr_age) = engine.with_mut(|engine| {
                    let borrowed = engine.borrow();
                    (
                        borrowed.underruns(),
                        borrowed.trimmed_samples(),
                        borrowed.clipped_samples(),
                        borrowed.nr_average_suppression_pct(),
                        borrowed.nr_fault_resets(),
                        borrowed.nr_capture_age_seconds(),
                    )
                });
                underruns.set(u);
                trimmed_samples.set(t);
                clipped_samples.set(c);
                nr_avg_suppression_pct.set(nr_pct);
                nr_fault_resets.set(nr_resets);
                nr_capture_age_seconds.set(nr_age);
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    });

    use_effect(move || {
        if let Some(loaded) = load_settings() {
            android_backend.set(loaded.android_backend.clamp(0, 2));
            android_burst.set(loaded.android_burst.clamp(64, 512));
            android_input_device_id.set(loaded.android_input_device_id.max(0));
            android_output_device_id.set(loaded.android_output_device_id.max(0));
            show_advanced.set(loaded.show_advanced);
            eq_editable.set(loaded.eq_editable);
            selected_profile.set(loaded.selected_profile_index.min(2));
            profile_name.set(loaded.selected_profile_name.clone());
            settings.set(loaded);
        }
    });

    use_effect(move || {
        let snapshot = settings();
        save_settings(&snapshot);
    });

    use_effect(move || {
        mic_permission.set(check_microphone_permission());
    });

    let can_toggle_dsp = use_memo(move || {
        #[cfg(target_os = "android")]
        {
            settings().profile_ready
        }
        #[cfg(not(target_os = "android"))]
        {
            true
        }
    });
    let dsp_status = use_memo(move || if settings().dsp_enabled { "On" } else { "Off" });
    let engine_status = use_memo(move || match engine_state() {
        EngineState::Running => "Running",
        EngineState::Stopped => "Stopped",
    });
    let sweep_progress = use_memo(move || {
        let state = sweep();
        if state.bands.is_empty() {
            0
        } else {
            let clamped = state.step.min(state.bands.len());
            (clamped * 100) / state.bands.len()
        }
    });
    let current_sweep_frequency = use_memo(move || {
        let state = sweep();
        state.bands.get(state.step).map(|band| band.frequency_hz)
    });
    let current_sweep_level = use_memo(move || {
        let state = sweep();
        state.bands.get(state.step).map(|band| band.current_db)
    });
    let input_percent = use_memo(move || input_peak().clamp(0.0, 1.0) * 100.0);
    let refresh_android_device_lists = Rc::new(RefCell::new({
        let mut android_input_devices = android_input_devices;
        let mut android_output_devices = android_output_devices;
        move || {
            #[cfg(target_os = "android")]
            {
                let mut input_devices = vec![(0, "Automatic".to_string())];
                input_devices.extend(crate::android_audio::list_input_devices());
                android_input_devices.set(input_devices);

                let mut output_devices = vec![(0, "Automatic".to_string())];
                output_devices.extend(crate::android_audio::list_output_devices());
                android_output_devices.set(output_devices);
            }
            #[cfg(not(target_os = "android"))]
            {
                android_input_devices.set(vec![(0, "Automatic".to_string())]);
                android_output_devices.set(vec![(0, "Automatic".to_string())]);
            }
        }
    }));
    let bt_mic_voicecomm_warning = use_memo(move || {
        if !cfg!(target_os = "android") {
            return false;
        }

        if route_status().contains("VoiceComm: On") {
            return true;
        }

        let selected_input = android_input_device_id();
        if selected_input <= 0 {
            return false;
        }

        android_input_devices()
            .iter()
            .find(|(device_id, _)| *device_id == selected_input)
            .map(|(_, label)| {
                let lower = label.to_ascii_lowercase();
                lower.contains("bluetooth") || lower.contains("ble headset")
            })
            .unwrap_or(false)
    });
    let profile_dirty = use_memo(move || {
        let snapshot = settings();
        let index = selected_profile();
        if let Some(profile) = snapshot.profiles.get(index) {
            !profile_matches_settings(profile, &snapshot, profile_name().trim(), index)
        } else {
            false
        }
    });
    let selected_profile_last_saved = use_memo(move || {
        let snapshot = settings();
        snapshot
            .profiles
            .get(selected_profile())
            .map(|profile| profile.last_saved_epoch_secs)
            .filter(|epoch| *epoch > 0)
    });
    let selected_profile_exists = use_memo(move || {
        let snapshot = settings();
        snapshot.profiles.get(selected_profile()).is_some()
    });
    let profile_change_summary = use_memo(move || {
        let snapshot = settings();
        let index = selected_profile();
        if let Some(profile) = snapshot.profiles.get(index) {
            let fields =
                changed_fields_for_profile(profile, &snapshot, profile_name().trim(), index);
            fields.join(", ")
        } else {
            String::new()
        }
    });
    let has_user_eq_offsets = use_memo(move || {
        settings()
            .user_eq_offsets
            .iter()
            .any(|band| band.value.abs() > 0.01)
    });
    let selected_profile_route_matches = use_memo(move || {
        let snapshot = settings();
        snapshot.profiles.get(selected_profile()).map(|profile| {
            profile.android_input_device_id == snapshot.android_input_device_id
                && profile.android_output_device_id == snapshot.android_output_device_id
        })
    });
    let refresh_android_device_lists_for_button = refresh_android_device_lists.clone();

    use_effect(move || {
        (refresh_android_device_lists.borrow_mut())();
    });

    let apply_profile = Rc::new(RefCell::new({
        let mut settings = settings;
        let mut android_backend = android_backend;
        let mut android_burst = android_burst;
        let mut android_input_device_id = android_input_device_id;
        let mut android_output_device_id = android_output_device_id;
        let mut engine = engine;
        move |profile: UserProfile| {
            settings.with_mut(|settings| {
                settings.eq_bands = profile.eq_bands.clone();
                settings.calibration_eq_bands = profile.calibration_eq_bands.clone();
                settings.user_eq_offsets = profile.user_eq_offsets.clone();
                settings.profile_ready = profile.profile_ready;
                settings.noise_strength = profile.noise_strength;
                settings.noise_cancel = profile.noise_cancel;
                settings.noise_profile_mode = profile.noise_profile_mode;
                settings.limiter_enabled = profile.limiter_enabled;
                settings.safe_mode = profile.safe_mode;
                settings.master_gain = profile.master_gain;
                settings.input_gain = profile.input_gain;
                settings.agc_enabled = profile.agc_enabled;
                settings.agc_max_gain = profile.agc_max_gain;
                settings.android_backend = profile.android_backend;
                settings.android_burst = profile.android_burst;
                settings.android_input_device_id = profile.android_input_device_id;
                settings.android_output_device_id = profile.android_output_device_id;
                settings.low_cut_hz = profile.low_cut_hz;
                settings.high_cut_hz = profile.high_cut_hz;
                settings.thresholds = profile.thresholds.clone();
            });
            android_backend.set(profile.android_backend.clamp(0, 2));
            android_burst.set(profile.android_burst.clamp(64, 512));
            android_input_device_id.set(profile.android_input_device_id.max(0));
            android_output_device_id.set(profile.android_output_device_id.max(0));
            engine.with_mut(|engine| {
                for (index, band) in profile.eq_bands.iter().enumerate() {
                    engine.borrow_mut().apply(DspCommand::SetEqBand {
                        index,
                        value_db: band.value,
                    });
                }
                engine
                    .borrow_mut()
                    .apply(DspCommand::SetNoiseStrength(profile.noise_strength));
                engine
                    .borrow_mut()
                    .apply(DspCommand::SetNoiseCancel(profile.noise_cancel));
                engine.borrow_mut().apply(DspCommand::SetNoiseProfileMode(
                    profile.noise_profile_mode.clamp(0, 1),
                ));
                engine
                    .borrow_mut()
                    .apply(DspCommand::SetLimiterEnabled(profile.limiter_enabled));
                engine
                    .borrow_mut()
                    .apply(DspCommand::SetSafeMode(profile.safe_mode));
                engine
                    .borrow_mut()
                    .apply(DspCommand::SetMasterGain(profile.master_gain));
                engine
                    .borrow_mut()
                    .apply(DspCommand::SetInputGain(profile.input_gain));
                engine
                    .borrow_mut()
                    .apply(DspCommand::SetAgcEnabled(profile.agc_enabled));
                engine
                    .borrow_mut()
                    .apply(DspCommand::SetAgcMaxGain(profile.agc_max_gain));
                engine
                    .borrow_mut()
                    .apply(DspCommand::SetLowCutHz(profile.low_cut_hz));
                engine
                    .borrow_mut()
                    .apply(DspCommand::SetHighCutHz(profile.high_cut_hz));
                engine
                    .borrow_mut()
                    .apply(DspCommand::SetAndroidAudioConfig {
                        backend: profile.android_backend.clamp(0, 2),
                        frames_per_burst: profile.android_burst.clamp(64, 512),
                    });
                engine
                    .borrow_mut()
                    .apply(DspCommand::SetAndroidPreferredDevices {
                        input_device_id: profile.android_input_device_id.max(0),
                        output_device_id: profile.android_output_device_id.max(0),
                    });
                let (low, mid, high) = derive_band_gains(&profile.eq_bands);
                engine
                    .borrow_mut()
                    .apply(DspCommand::SetBandGains { low, mid, high });
            });
        }
    }));

    use_effect(move || {
        let backend = android_backend().clamp(0, 2);
        let burst = android_burst().clamp(64, 512);
        engine.with_mut(|engine| {
            engine
                .borrow_mut()
                .apply(DspCommand::SetAndroidAudioConfig {
                    backend,
                    frames_per_burst: burst,
                });
        });
        settings.with_mut(|settings| {
            settings.android_backend = backend;
            settings.android_burst = burst;
        });
    });
    use_effect(move || {
        let input_devices = android_input_devices();
        let output_devices = android_output_devices();
        if input_devices.len() <= 1 || output_devices.len() <= 1 {
            return;
        }
        let mut input_id = android_input_device_id();
        let mut output_id = android_output_device_id();

        if !input_devices.iter().any(|(id, _)| *id == input_id) {
            input_id = 0;
            android_input_device_id.set(0);
        }
        if !output_devices.iter().any(|(id, _)| *id == output_id) {
            output_id = 0;
            android_output_device_id.set(0);
        }

        engine.with_mut(|engine| {
            engine
                .borrow_mut()
                .apply(DspCommand::SetAndroidPreferredDevices {
                    input_device_id: input_id,
                    output_device_id: output_id,
                });
        });
        settings.with_mut(|settings| {
            settings.android_input_device_id = input_id;
            settings.android_output_device_id = output_id;
        });
    });

    use_effect(move || {
        let strength = settings().noise_strength;
        engine.with_mut(|engine| {
            engine
                .borrow_mut()
                .apply(DspCommand::SetNoiseStrength(strength));
        });
    });

    use_effect(move || {
        let mode = settings().noise_profile_mode.clamp(0, 1);
        engine.with_mut(|engine| {
            engine
                .borrow_mut()
                .apply(DspCommand::SetNoiseProfileMode(mode));
        });
    });

    use_effect(move || {
        let enabled = settings().limiter_enabled;
        engine.with_mut(|engine| {
            engine
                .borrow_mut()
                .apply(DspCommand::SetLimiterEnabled(enabled));
        });
    });

    use_effect(move || {
        let enabled = settings().safe_mode;
        engine.with_mut(|engine| {
            engine.borrow_mut().apply(DspCommand::SetSafeMode(enabled));
        });
    });

    use_effect(move || {
        let gain = settings().master_gain;
        engine.with_mut(|engine| {
            engine.borrow_mut().apply(DspCommand::SetMasterGain(gain));
        });
    });

    use_effect(move || {
        let gain = settings().input_gain;
        engine.with_mut(|engine| {
            engine.borrow_mut().apply(DspCommand::SetInputGain(gain));
        });
    });

    use_effect(move || {
        let enabled = settings().agc_enabled;
        engine.with_mut(|engine| {
            engine
                .borrow_mut()
                .apply(DspCommand::SetAgcEnabled(enabled));
        });
    });

    use_effect(move || {
        let gain = settings().agc_max_gain;
        engine.with_mut(|engine| {
            engine.borrow_mut().apply(DspCommand::SetAgcMaxGain(gain));
        });
    });

    use_effect(move || {
        let low_cut = settings().low_cut_hz;
        engine.with_mut(|engine| {
            engine.borrow_mut().apply(DspCommand::SetLowCutHz(low_cut));
        });
    });

    use_effect(move || {
        let high_cut = settings().high_cut_hz;
        engine.with_mut(|engine| {
            engine
                .borrow_mut()
                .apply(DspCommand::SetHighCutHz(high_cut));
        });
    });

    use_effect(move || {
        let bands = settings().eq_bands;
        engine.with_mut(|engine| {
            for (index, band) in bands.iter().enumerate() {
                engine.borrow_mut().apply(DspCommand::SetEqBand {
                    index,
                    value_db: band.value,
                });
            }
        });
        let (low, mid, high) = derive_band_gains(&bands);
        engine.with_mut(|engine| {
            engine
                .borrow_mut()
                .apply(DspCommand::SetBandGains { low, mid, high });
        });
    });

    let play_current_tone = Rc::new(RefCell::new({
        let mut engine = engine;
        move || {
            if let (Some(freq), Some(level_db)) = (current_sweep_frequency(), current_sweep_level())
            {
                let reference_hz = 1000.0f32;
                let freq_hz = freq as f32;
                let tilt = (reference_hz / freq_hz).powf(0.35).clamp(0.65, 1.6);
                let amplitude = (0.35 * 10f32.powf(level_db / 20.0) * tilt).clamp(0.0, 0.6);
                engine.with_mut(|engine| {
                    engine
                        .borrow_mut()
                        .apply(DspCommand::AdvanceSweep(SweepStep {
                            frequency_hz: freq,
                            amplitude,
                        }));
                });
            }
        }
    }));

    let play_current_tone_for_apply = play_current_tone.clone();
    let apply_sweep_response = Rc::new(RefCell::new(move |heard: bool| {
        let (Some(freq), Some(level_db)) = (current_sweep_frequency(), current_sweep_level())
        else {
            return;
        };

        settings.with_mut(|settings| {
            settings.sweep_results.push(SweepResult {
                frequency_hz: freq,
                level_db,
                heard,
            });
        });

        sweep.with_mut(|state| {
            let mut finalize_threshold: Option<f32> = None;

            {
                let Some(band) = state.bands.get_mut(state.step) else {
                    return;
                };

                if let Some(last) = band.last_heard {
                    if last != heard {
                        band.reversals = band.reversals.saturating_add(1);
                        if band.reversals == 1 {
                            band.step_db = 2.0;
                        }
                    }
                }
                band.last_heard = Some(heard);

                let should_finalize = (band.reversals >= 2 && band.step_db <= 2.0)
                    || (!heard && band.current_db >= -5.0)
                    || (heard && band.current_db <= -80.0);
                if should_finalize {
                    finalize_threshold = Some(band.current_db);
                } else {
                    let mut next_db = if heard {
                        band.current_db - band.step_db
                    } else {
                        band.current_db + band.step_db
                    };
                    next_db = next_db.clamp(-80.0, -5.0);
                    band.current_db = next_db;
                }
            }

            let Some(threshold_db) = finalize_threshold else {
                return;
            };

            settings.with_mut(|settings| {
                settings
                    .thresholds
                    .retain(|threshold| threshold.frequency_hz != freq);
                settings.thresholds.push(crate::state::HearingThreshold {
                    frequency_hz: freq,
                    threshold_db,
                });

                settings.calibration_eq_bands = derive_eq_from_sweep(
                    &settings.calibration_eq_bands,
                    &settings.sweep_results,
                    &settings.thresholds,
                );
                settings.eq_bands =
                    combine_eq_layers(&settings.calibration_eq_bands, &settings.user_eq_offsets);
                let updated_bands = settings.eq_bands.clone();
                engine.with_mut(|engine| {
                    for (index, band) in updated_bands.iter().enumerate() {
                        engine.borrow_mut().apply(DspCommand::SetEqBand {
                            index,
                            value_db: band.value,
                        });
                    }
                    let (low, mid, high) = derive_band_gains(&updated_bands);
                    engine
                        .borrow_mut()
                        .apply(DspCommand::SetBandGains { low, mid, high });
                });

                if state.step + 1 >= state.bands.len() {
                    state.running = false;
                    state.completed = true;
                    calibration_mode.set(false);
                    settings.profile_ready = true;
                    engine.with_mut(|engine| {
                        engine.borrow_mut().apply(DspCommand::StopSweep);
                    });
                } else {
                    state.step += 1;
                }
            });
        });

        missed_responses.set(0);

        if sweep().running {
            (play_current_tone_for_apply.borrow_mut())();
        }
    }));
    let heard_response = apply_sweep_response.clone();
    let not_heard_response = apply_sweep_response.clone();

    let sweep_for_future = sweep;
    let play_current_tone_for_future = play_current_tone.clone();
    use_future(move || {
        let sweep = sweep_for_future;
        let play_current_tone = play_current_tone_for_future.clone();
        let mut missed_responses = missed_responses;
        let auto_response = apply_sweep_response.clone();
        async move {
            loop {
                if sweep().running {
                    (play_current_tone.borrow_mut())();
                    let next_missed = missed_responses() + 1;
                    missed_responses.set(next_missed);
                    if next_missed >= 3 {
                        let handler = auto_response.clone();
                        (handler.borrow_mut())(false);
                    }
                    tokio::time::sleep(Duration::from_millis(1200)).await;
                } else {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
        }
    });

    use_future(move || {
        let mut noise_capture = noise_capture;
        async move {
            loop {
                if noise_capture() {
                    tokio::time::sleep(Duration::from_millis(NOISE_CAPTURE_UI_MS)).await;
                    noise_capture.set(false);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    });

    let play_current_tone_for_calibration_start = play_current_tone.clone();
    let play_current_tone_for_replay_button = play_current_tone.clone();
    let apply_profile_for_load = apply_profile.clone();
    let apply_profile_for_revert = apply_profile.clone();

    rsx! {
        if mic_permission() != PermissionState::Granted {
            div { class: "permission-screen",
                div { class: "permission-card",
                    h1 { "Microphone access required" }
                    p {
                        "HearBuds needs access to your microphone to create a hearing profile and run DSP."
                    }
                    button {
                        class: "primary",
                        onclick: move |_| {
                            request_microphone_permission();
                            let mut mic_permission = mic_permission;
                            spawn(async move {
                                for _ in 0..10 {
                                    let state = check_microphone_permission();
                                    mic_permission.set(state);
                                    if state == PermissionState::Granted {
                                        break;
                                    }
                                    tokio::time::sleep(Duration::from_millis(300)).await;
                                }
                            });
                        },
                        "Grant microphone access"
                    }
                    p { class: "helper",
                        "If you denied access, enable the microphone permission in system settings and try again."
                    }
                }
            }
        } else {
            div { id: "app",
                header { class: "app-header",
                    div { class: "title",
                        p { class: "eyebrow", "HearBuds" }
                        h1 { "Pocket hearing aid." }
                        p { class: "subtitle",
                            "Run the sweep once, then keep DSP on for real-time listening."
                        }
                    }
                        div { class: "status-card",
                            div { class: "status-line",
                                span { class: "status-label", "DSP" }
                            span {
                                class: if settings().dsp_enabled { "status-pill on" } else { "status-pill off" },
                                "{dsp_status}"
                            }
                        }
                        div { class: "status-line",
                            span { class: "status-label", "Profile" }
                            span {
                                class: if settings().profile_ready { "status-pill ready" } else { "status-pill pending" },
                                if settings().profile_ready { "Ready" } else { "Calibrate" }
                            }
                        }
                        div { class: "status-line",
                            span { class: "status-label", "Engine" }
                            span {
                                class: if engine_state() == EngineState::Running { "status-pill on" } else { "status-pill off" },
                                "{engine_status}"
                            }
                        }
                        if cfg!(target_os = "android") {
                            div { class: "status-line compact",
                                span { class: "status-label", "Route" }
                                span { class: "helper", "{route_status()}" }
                            }
                        }
                        if show_advanced() {
                            div { class: "status-line compact",
                                span { class: "status-label", "Input" }
                                div { class: "level-meter",
                                    div {
                                        class: "level-fill",
                                        style: "width: {input_percent}%;",
                                    }
                                }
                            }
                            div { class: "status-line compact",
                                span { class: "status-label", "Callback" }
                                span { class: "status-pill ready", "{callback_ms():.2} ms" }
                            }
                            div { class: "status-line compact",
                                span { class: "status-label", "Underruns" }
                                span { class: "status-pill pending", "{underruns()}" }
                            }
                            div { class: "status-line compact",
                                span { class: "status-label", "Trimmed" }
                                span { class: "status-pill pending", "{trimmed_samples()}" }
                            }
                            div { class: "status-line compact",
                                span { class: "status-label", "Clipped" }
                                span { class: "status-pill pending", "{clipped_samples()}" }
                            }
                            div { class: "status-line compact",
                                span { class: "status-label", "NR suppress" }
                                span { class: "status-pill ready", "{nr_avg_suppression_pct():.0}%" }
                            }
                            div { class: "status-line compact",
                                span { class: "status-label", "NR resets" }
                                span { class: "status-pill pending", "{nr_fault_resets()}" }
                            }
                            div { class: "status-line compact",
                                span { class: "status-label", "Noise age" }
                                span { class: "status-pill pending", "{nr_capture_age_seconds():.0}s" }
                            }
                        }
                    }
                }

                section { class: "panel controls",
                    div { class: "panel-header",
                        h2 { "Quick controls" }
                        button {
                            class: if show_advanced() { "ghost active" } else { "ghost" },
                            onclick: move |_| {
                                let next_value = !show_advanced();
                                show_advanced.set(next_value);
                                settings.with_mut(|settings| {
                                    settings.show_advanced = next_value;
                                });
                            },
                            if show_advanced() { "Hide advanced" } else { "Show advanced" }
                        }
                    }
                    div { class: "control-row",
                        button {
                            class: "primary",
                            disabled: !can_toggle_dsp(),
                            onclick: move |_| {
                                let should_start = !settings().dsp_enabled;
                                engine.with_mut(|engine| {
                                    if should_start {
                                        let snapshot = settings();
                                        engine.borrow_mut().apply(DspCommand::SetAndroidAudioConfig {
                                            backend: snapshot.android_backend.clamp(0, 2),
                                            frames_per_burst: snapshot.android_burst.clamp(64, 512),
                                        });
                                        engine
                                            .borrow_mut()
                                            .apply(DspCommand::SetAndroidPreferredDevices {
                                                input_device_id: snapshot.android_input_device_id.max(0),
                                                output_device_id: snapshot.android_output_device_id.max(0),
                                            });
                                        engine
                                            .borrow_mut()
                                            .apply(DspCommand::SetNoiseStrength(snapshot.noise_strength));
                                        engine
                                            .borrow_mut()
                                            .apply(DspCommand::SetNoiseCancel(snapshot.noise_cancel));
                                        engine.borrow_mut().apply(DspCommand::SetNoiseProfileMode(
                                            snapshot.noise_profile_mode.clamp(0, 1),
                                        ));
                                        engine
                                            .borrow_mut()
                                            .apply(DspCommand::SetLimiterEnabled(snapshot.limiter_enabled));
                                        engine
                                            .borrow_mut()
                                            .apply(DspCommand::SetSafeMode(snapshot.safe_mode));
                                        engine
                                            .borrow_mut()
                                            .apply(DspCommand::SetMasterGain(snapshot.master_gain));
                                        engine
                                            .borrow_mut()
                                            .apply(DspCommand::SetInputGain(snapshot.input_gain));
                                        engine
                                            .borrow_mut()
                                            .apply(DspCommand::SetAgcEnabled(snapshot.agc_enabled));
                                        engine
                                            .borrow_mut()
                                            .apply(DspCommand::SetAgcMaxGain(snapshot.agc_max_gain));
                                        engine
                                            .borrow_mut()
                                            .apply(DspCommand::SetLowCutHz(snapshot.low_cut_hz));
                                        engine
                                            .borrow_mut()
                                            .apply(DspCommand::SetHighCutHz(snapshot.high_cut_hz));
                                        for (index, band) in snapshot.eq_bands.iter().enumerate() {
                                            engine.borrow_mut().apply(DspCommand::SetEqBand {
                                                index,
                                                value_db: band.value,
                                            });
                                        }
                                        let (low, mid, high) = derive_band_gains(&snapshot.eq_bands);
                                        engine
                                            .borrow_mut()
                                            .apply(DspCommand::SetBandGains { low, mid, high });
                                        engine.borrow_mut().apply(DspCommand::Start);
                                    } else {
                                        engine.borrow_mut().apply(DspCommand::Stop);
                                    }
                                });
                                let actual_state = engine.with_mut(|engine| engine.borrow().state());
                                settings.with_mut(|settings| {
                                    settings.dsp_enabled = actual_state == EngineState::Running;
                                });
                                engine_state.set(actual_state);
                            },
                            if settings().dsp_enabled { "DSP On" } else { "DSP Off" }
                        }
                    }
                    div { class: "control-grid",
                        div { class: "control-card",
                            h3 { "Noise reduction" }
                            label { class: "toggle",
                                input {
                                    r#type: "checkbox",
                                    checked: settings().noise_cancel,
                                    onchange: move |_| {
                                        settings.with_mut(|settings| {
                                            settings.noise_cancel = !settings.noise_cancel;
                                        });
                                        engine.with_mut(|engine| {
                                            engine
                                                .borrow_mut()
                                                .apply(DspCommand::SetNoiseCancel(settings().noise_cancel));
                                            engine
                                                .borrow_mut()
                                                .apply(DspCommand::SetNoiseStrength(settings().noise_strength));
                                        });
                                    },
                                }
                                span { if settings().noise_cancel { "On" } else { "Off" } }
                            }
                            if show_advanced() {
                                div { class: "noise-strength compact",
                                    span { class: "status-label", "Profile" }
                                    select {
                                        value: "{settings().noise_profile_mode}",
                                        onchange: move |event| {
                                            if let Ok(parsed) = event.value().parse::<i32>() {
                                                let mode = parsed.clamp(0, 1);
                                                settings.with_mut(|settings| {
                                                    settings.noise_profile_mode = mode;
                                                });
                                                engine.with_mut(|engine| {
                                                    engine
                                                        .borrow_mut()
                                                        .apply(DspCommand::SetNoiseProfileMode(mode));
                                                });
                                            }
                                        },
                                        option { value: "0", "Comfort" }
                                        option { value: "1", "Strong" }
                                    }
                                }
                                div { class: "noise-strength compact",
                                    span { class: "status-label", "Strength" }
                                    input {
                                        r#type: "range",
                                        min: "0",
                                        max: "100",
                                        step: "1",
                                        value: "{(settings().noise_strength * 100.0).round()}",
                                        disabled: !settings().noise_cancel,
                                        oninput: move |event| {
                                            if let Ok(parsed) = event.value().parse::<f32>() {
                                                let strength = (parsed / 100.0).clamp(0.0, 1.0);
                                                settings.with_mut(|settings| {
                                                    settings.noise_strength = strength;
                                                });
                                                engine.with_mut(|engine| {
                                                    engine.borrow_mut().apply(DspCommand::SetNoiseStrength(strength));
                                                });
                                            }
                                        },
                                    }
                                    span { class: "helper", "{(settings().noise_strength * 100.0).round()}%" }
                                }
                                button {
                                    class: "secondary",
                                    disabled: !settings().noise_cancel || noise_capture(),
                                    onclick: move |_| {
                                        noise_capture.set(true);
                                        engine.with_mut(|engine| {
                                            engine.borrow_mut().apply(DspCommand::CaptureNoiseProfile);
                                        });
                                    },
                                    if noise_capture() { "Capturing..." } else { "Capture room noise" }
                                }
                                if noise_capture() {
                                    p { class: "helper", "Stay quiet for about 2 seconds." }
                                }
                            }
                        }
                        if show_advanced() {
                            div { class: "advanced-divider",
                                span { "Advanced settings" }
                            }
                            div { class: "advanced-settings",
                                if cfg!(target_os = "android") {
                                div { class: "control-card",
                                    h3 { "Audio device setup" }
                                    label { class: "helper", "Audio engine" }
                                    select {
                                        value: "{android_backend()}",
                                        onchange: move |event| {
                                            if let Ok(parsed) = event.value().parse::<i32>() {
                                                let backend = parsed.clamp(0, 2);
                                                android_backend.set(backend);
                                                settings.with_mut(|settings| {
                                                    settings.android_backend = backend;
                                                });
                                                let burst = android_burst();
                                                engine.with_mut(|engine| {
                                                    engine.borrow_mut().apply(DspCommand::SetAndroidAudioConfig {
                                                        backend,
                                                        frames_per_burst: burst,
                                                    });
                                                });
                                            }
                                        },
                                        option { value: "0", "Automatic (Recommended)" }
                                        option { value: "1", "Low-latency (AAudio)" }
                                        option { value: "2", "Compatibility (OpenSL ES)" }
                                    }
                                    label { class: "helper", "Latency tuning" }
                                    input {
                                        r#type: "range",
                                        min: "64",
                                        max: "512",
                                        step: "32",
                                        value: "{android_burst()}",
                                        oninput: move |event| {
                                            if let Ok(parsed) = event.value().parse::<u32>() {
                                                let burst = parsed.clamp(64, 512);
                                                android_burst.set(burst);
                                                settings.with_mut(|settings| {
                                                    settings.android_burst = burst;
                                                });
                                                let backend = android_backend();
                                                engine.with_mut(|engine| {
                                                    engine.borrow_mut().apply(DspCommand::SetAndroidAudioConfig {
                                                        backend,
                                                        frames_per_burst: burst,
                                                    });
                                                });
                                            }
                                        },
                                    }
                                    span { class: "helper", "{android_burst()} frames per callback" }
                                    label { class: "helper", "Microphone" }
                                    select {
                                        value: "{android_input_device_id()}",
                                        onchange: move |event| {
                                            if let Ok(parsed) = event.value().parse::<i32>() {
                                                android_input_device_id.set(parsed.max(0));
                                                settings.with_mut(|settings| {
                                                    settings.android_input_device_id = parsed.max(0);
                                                });
                                                engine.with_mut(|engine| {
                                                    engine.borrow_mut().apply(
                                                        DspCommand::SetAndroidPreferredDevices {
                                                            input_device_id: parsed.max(0),
                                                            output_device_id: android_output_device_id(),
                                                        },
                                                    );
                                                });
                                            }
                                        },
                                        for (device_id, label) in android_input_devices().iter() {
                                            option { key: "{device_id}", value: "{device_id}", "{label}" }
                                        }
                                    }
                                    if bt_mic_voicecomm_warning() {
                                        p { class: "helper",
                                            "Bluetooth mic routes usually force call mode (VoiceComm), which can add hiss and reduce fidelity. Use phone mic + A2DP output for best quality."
                                        }
                                    }
                                    label { class: "helper", "Speaker / Earbuds" }
                                    select {
                                        value: "{android_output_device_id()}",
                                        onchange: move |event| {
                                            if let Ok(parsed) = event.value().parse::<i32>() {
                                                android_output_device_id.set(parsed.max(0));
                                                settings.with_mut(|settings| {
                                                    settings.android_output_device_id = parsed.max(0);
                                                });
                                                engine.with_mut(|engine| {
                                                    engine.borrow_mut().apply(
                                                        DspCommand::SetAndroidPreferredDevices {
                                                            input_device_id: android_input_device_id(),
                                                            output_device_id: parsed.max(0),
                                                        },
                                                    );
                                                });
                                            }
                                        },
                                        for (device_id, label) in android_output_devices().iter() {
                                            option { key: "{device_id}", value: "{device_id}", "{label}" }
                                        }
                                    }
                                    button {
                                        class: "ghost",
                                        onclick: move |_| {
                                            (refresh_android_device_lists_for_button.borrow_mut())();
                                        },
                                        "Refresh device list"
                                    }
                                }
                            }
                            div { class: "control-card",
                                h3 { "Input gain" }
                                input {
                                    r#type: "range",
                                    min: "0.5",
                                    max: "20.0",
                                    step: "0.1",
                                    value: "{settings().input_gain}",
                                    oninput: move |event| {
                                        if let Ok(parsed) = event.value().parse::<f32>() {
                                            let gain = parsed.clamp(0.5, 20.0);
                                            settings.with_mut(|settings| {
                                                settings.input_gain = gain;
                                            });
                                            engine.with_mut(|engine| {
                                                engine.borrow_mut().apply(DspCommand::SetInputGain(gain));
                                            });
                                        }
                                    },
                                }
                                span { class: "helper", "{settings().input_gain:.2}x" }
                            }
                            div { class: "control-card",
                                h3 { "Auto gain (AGC)" }
                                label { class: "toggle",
                                    input {
                                        r#type: "checkbox",
                                        checked: settings().agc_enabled,
                                        onchange: move |_| {
                                            let next_value = !settings().agc_enabled;
                                            settings.with_mut(|settings| {
                                                settings.agc_enabled = next_value;
                                            });
                                            engine.with_mut(|engine| {
                                                engine
                                                    .borrow_mut()
                                                    .apply(DspCommand::SetAgcEnabled(next_value));
                                            });
                                        },
                                    }
                                    span { if settings().agc_enabled { "On" } else { "Off" } }
                                }
                                input {
                                    r#type: "range",
                                    min: "1.0",
                                    max: "20.0",
                                    step: "0.5",
                                    value: "{settings().agc_max_gain}",
                                    disabled: !settings().agc_enabled,
                                    oninput: move |event| {
                                        if let Ok(parsed) = event.value().parse::<f32>() {
                                            let gain = parsed.clamp(1.0, 20.0);
                                            settings.with_mut(|settings| {
                                                settings.agc_max_gain = gain;
                                            });
                                            engine.with_mut(|engine| {
                                                engine.borrow_mut().apply(DspCommand::SetAgcMaxGain(gain));
                                            });
                                        }
                                    },
                                }
                                span { class: "helper", "Max gain {settings().agc_max_gain:.1}x" }
                            }
                            div { class: "control-card",
                                h3 { "Output level" }
                                input {
                                    r#type: "range",
                                    min: "0.5",
                                    max: "6.0",
                                    step: "0.05",
                                    value: "{settings().master_gain}",
                                    oninput: move |event| {
                                        if let Ok(parsed) = event.value().parse::<f32>() {
                                            let gain = parsed.clamp(0.5, 6.0);
                                            settings.with_mut(|settings| {
                                                settings.master_gain = gain;
                                            });
                                            engine.with_mut(|engine| {
                                                engine.borrow_mut().apply(DspCommand::SetMasterGain(gain));
                                            });
                                        }
                                    },
                                }
                                span { class: "helper", "{settings().master_gain:.2}x" }
                            }
                            div { class: "control-card",
                                h3 { "Tone shaping" }
                                label { class: "helper", "Low cut (rumble control)" }
                                input {
                                    r#type: "range",
                                    min: "20",
                                    max: "400",
                                    step: "5",
                                    value: "{settings().low_cut_hz}",
                                    oninput: move |event| {
                                        if let Ok(parsed) = event.value().parse::<f32>() {
                                            settings.with_mut(|settings| {
                                                settings.low_cut_hz = parsed.clamp(20.0, 400.0);
                                            });
                                        }
                                    },
                                }
                                span { class: "helper", "{settings().low_cut_hz.round()} Hz" }
                                label { class: "helper", "High cut (hiss control)" }
                                input {
                                    r#type: "range",
                                    min: "2000",
                                    max: "12000",
                                    step: "100",
                                    value: "{settings().high_cut_hz}",
                                    oninput: move |event| {
                                        if let Ok(parsed) = event.value().parse::<f32>() {
                                            settings.with_mut(|settings| {
                                                settings.high_cut_hz = parsed.clamp(2000.0, 12_000.0);
                                            });
                                        }
                                    },
                                }
                                span { class: "helper", "{settings().high_cut_hz.round()} Hz" }
                            }
                            div { class: "control-card",
                                h3 { "Safety" }
                                label { class: "toggle",
                                    input {
                                        r#type: "checkbox",
                                        checked: settings().limiter_enabled,
                                        onchange: move |_| {
                                            let next_value = !settings().limiter_enabled;
                                            settings.with_mut(|settings| {
                                                settings.limiter_enabled = next_value;
                                            });
                                            engine.with_mut(|engine| {
                                                engine
                                                    .borrow_mut()
                                                    .apply(DspCommand::SetLimiterEnabled(next_value));
                                            });
                                        },
                                }
                                span { if settings().limiter_enabled { "Limiter on" } else { "Limiter off" } }
                            }
                            p { class: "helper", "Reduces feedback spikes." }
                            label { class: "toggle",
                                input {
                                    r#type: "checkbox",
                                    checked: settings().safe_mode,
                                    onchange: move |_| {
                                        let next_value = !settings().safe_mode;
                                        settings.with_mut(|settings| {
                                            settings.safe_mode = next_value;
                                        });
                                        engine.with_mut(|engine| {
                                            engine
                                                .borrow_mut()
                                                .apply(DspCommand::SetSafeMode(next_value));
                                        });
                                    },
                                }
                                span { if settings().safe_mode { "Safe mode on" } else { "Safe mode off" } }
                            }
                        }
                            div { class: "control-card",
                                h3 { "Advanced defaults" }
                                p { class: "helper", "Reset advanced audio tuning to recommended defaults while keeping your selected mic and output devices." }
                                button {
                                    class: "ghost",
                                    onclick: move |_| {
                                        let defaults = AppSettings::default();
                                        settings.with_mut(|settings| {
                                            settings.noise_strength = defaults.noise_strength;
                                            settings.noise_profile_mode = defaults.noise_profile_mode;
                                            settings.limiter_enabled = defaults.limiter_enabled;
                                            settings.safe_mode = defaults.safe_mode;
                                            settings.master_gain = defaults.master_gain;
                                            settings.input_gain = defaults.input_gain;
                                            settings.agc_enabled = defaults.agc_enabled;
                                            settings.agc_max_gain = defaults.agc_max_gain;
                                            settings.low_cut_hz = defaults.low_cut_hz;
                                            settings.high_cut_hz = defaults.high_cut_hz;
                                        });
                                        android_backend.set(defaults.android_backend);
                                        android_burst.set(defaults.android_burst);

                                        engine.with_mut(|engine| {
                                            engine.borrow_mut().apply(DspCommand::SetNoiseStrength(defaults.noise_strength));
                                            engine.borrow_mut().apply(DspCommand::SetNoiseProfileMode(defaults.noise_profile_mode));
                                            engine.borrow_mut().apply(DspCommand::SetLimiterEnabled(defaults.limiter_enabled));
                                            engine.borrow_mut().apply(DspCommand::SetSafeMode(defaults.safe_mode));
                                            engine.borrow_mut().apply(DspCommand::SetMasterGain(defaults.master_gain));
                                            engine.borrow_mut().apply(DspCommand::SetInputGain(defaults.input_gain));
                                            engine.borrow_mut().apply(DspCommand::SetAgcEnabled(defaults.agc_enabled));
                                            engine.borrow_mut().apply(DspCommand::SetAgcMaxGain(defaults.agc_max_gain));
                                            engine.borrow_mut().apply(DspCommand::SetLowCutHz(defaults.low_cut_hz));
                                            engine.borrow_mut().apply(DspCommand::SetHighCutHz(defaults.high_cut_hz));
                                            engine.borrow_mut().apply(DspCommand::SetAndroidAudioConfig {
                                                backend: defaults.android_backend,
                                                frames_per_burst: defaults.android_burst,
                                            });
                                        });
                                    },
                                    "Reset advanced defaults"
                                }
                            }
                            div { class: "control-card",
                                h3 { "Profiles" }
                                input {
                                    r#type: "text",
                                    value: "{profile_name()}",
                                    oninput: move |event| {
                                        let next_name = event.value();
                                        pending_overwrite_index.set(None);
                                        profile_feedback.set(String::new());
                                        profile_feedback_class.set(String::from("success-text"));
                                        profile_name.set(next_name.clone());
                                        settings.with_mut(|settings| {
                                            settings.selected_profile_name = next_name;
                                        });
                                    },
                                    placeholder: "Profile name"
                                }
                                select {
                                    value: "{selected_profile()}",
                                    onchange: move |event| {
                                        if let Ok(parsed) = event.value().parse::<usize>() {
                                            let selected_index = parsed.min(2);
                                            let snapshot = settings();
                                            let selected_name =
                                                if let Some(profile) = snapshot.profiles.get(selected_index) {
                                                    profile.name.clone()
                                                } else {
                                                    format!("Profile {}", selected_index + 1)
                                                };
                                            profile_name.set(selected_name.clone());
                                            settings.with_mut(|settings| {
                                                settings.selected_profile_index = selected_index;
                                                settings.selected_profile_name = selected_name.clone();
                                            });
                                            selected_profile.set(selected_index);
                                            pending_overwrite_index.set(None);
                                            profile_feedback.set(String::new());
                                            profile_feedback_class.set(String::from("success-text"));
                                        }
                                    },
                                    for (index, profile) in settings.read().profiles.iter().enumerate() {
                                        option { value: "{index}", "{profile.name}" }
                                    }
                                }
                                div { class: "control-row",
                                    button {
                                        class: "secondary",
                                        onclick: move |_| {
                                            let snapshot = settings();
                                            let index = selected_profile();
                                            let replacing_existing = snapshot.profiles.len() > index;
                                            if replacing_existing && pending_overwrite_index() != Some(index) {
                                                pending_overwrite_index.set(Some(index));
                                                profile_feedback.set(String::from("Confirm overwrite by pressing Save again."));
                                                profile_feedback_class.set(String::from("warning-text"));
                                                return;
                                            }
                                            let now = now_unix_epoch_secs();
                                            let name = normalized_profile_name(profile_name().trim(), index);
                                            let persisted_name = name.clone();
                                            settings.with_mut(|settings| {
                                                let profile = UserProfile {
                                                    name: name.clone(),
                                                    last_saved_epoch_secs: now,
                                                    profile_ready: snapshot.profile_ready,
                                                    eq_bands: snapshot.eq_bands.clone(),
                                                    noise_strength: snapshot.noise_strength,
                                                    noise_cancel: snapshot.noise_cancel,
                                                    noise_profile_mode: snapshot.noise_profile_mode,
                                                    limiter_enabled: snapshot.limiter_enabled,
                                                    safe_mode: snapshot.safe_mode,
                                                    master_gain: snapshot.master_gain,
                                                    input_gain: snapshot.input_gain,
                                                    agc_enabled: snapshot.agc_enabled,
                                                    agc_max_gain: snapshot.agc_max_gain,
                                                    android_backend: snapshot.android_backend,
                                                    android_burst: snapshot.android_burst,
                                                    android_input_device_id: snapshot.android_input_device_id,
                                                    android_output_device_id: snapshot.android_output_device_id,
                                                    calibration_eq_bands: snapshot.calibration_eq_bands.clone(),
                                                    user_eq_offsets: snapshot.user_eq_offsets.clone(),
                                                    low_cut_hz: snapshot.low_cut_hz,
                                                    high_cut_hz: snapshot.high_cut_hz,
                                                    thresholds: snapshot.thresholds.clone(),
                                                };
                                                if settings.profiles.len() > index {
                                                    settings.profiles[index] = profile;
                                                } else if settings.profiles.len() < 3 {
                                                    settings.profiles.push(profile);
                                                }
                                            });
                                            profile_name.set(name);
                                            settings.with_mut(|settings| {
                                                settings.selected_profile_index = index;
                                                settings.selected_profile_name = persisted_name.clone();
                                            });
                                            pending_overwrite_index.set(None);
                                            profile_feedback.set(String::from("Profile saved."));
                                            profile_feedback_class.set(String::from("success-text"));
                                        },
                                        "Save"
                                    }
                                    button {
                                        class: "ghost",
                                        disabled: !selected_profile_exists(),
                                        onclick: move |_| {
                                            let apply_profile = apply_profile_for_load.clone();
                                            pending_overwrite_index.set(None);
                                            let snapshot = settings();
                                            let profiles = snapshot.profiles.clone();
                                            let index = selected_profile();
                                            if let Some(profile) = profiles.get(index) {
                                                (apply_profile.borrow_mut())(profile.clone());
                                                profile_name.set(profile.name.clone());
                                                settings.with_mut(|settings| {
                                                    settings.selected_profile_name = profile.name.clone();
                                                });
                                                profile_feedback.set(String::from("Profile loaded."));
                                                profile_feedback_class.set(String::from("success-text"));
                                            } else {
                                                profile_feedback.set(String::from("No profile in selected slot."));
                                                profile_feedback_class.set(String::from("error-text"));
                                            }
                                        },
                                        "Load"
                                    }
                                    button {
                                        class: "ghost",
                                        disabled: !selected_profile_exists() || !profile_dirty(),
                                        onclick: move |_| {
                                            let apply_profile = apply_profile_for_revert.clone();
                                            pending_overwrite_index.set(None);
                                            let snapshot = settings();
                                            if let Some(profile) = snapshot.profiles.get(selected_profile()) {
                                                (apply_profile.borrow_mut())(profile.clone());
                                                profile_name.set(profile.name.clone());
                                                settings.with_mut(|settings| {
                                                    settings.selected_profile_name = profile.name.clone();
                                                });
                                                profile_feedback.set(String::from("Reverted unsaved changes."));
                                                profile_feedback_class.set(String::from("success-text"));
                                            }
                                        },
                                        "Revert tweaks"
                                    }
                                }
                                if profile_dirty() {
                                    p { class: "helper warning-text", "Unsaved changes for selected profile." }
                                    if !profile_change_summary().is_empty() {
                                        p { class: "helper", "Changed: {profile_change_summary()}" }
                                    }
                                }
                                if pending_overwrite_index() == Some(selected_profile()) {
                                    p { class: "helper warning-text", "Press Save again to overwrite this profile." }
                                }
                                if let Some(route_matches) = selected_profile_route_matches() {
                                    div { class: "status-line compact",
                                        span { class: "status-label", "Saved route" }
                                        span {
                                            class: if route_matches { "status-pill ready" } else { "status-pill pending" },
                                            if route_matches { "Match" } else { "Different" }
                                        }
                                    }
                                    if let Some(profile) = settings().profiles.get(selected_profile()) {
                                        p {
                                            class: "helper",
                                            "Saved: Mic {device_label_for_id(&android_input_devices(), profile.android_input_device_id)} -> Output {device_label_for_id(&android_output_devices(), profile.android_output_device_id)}"
                                        }
                                        p {
                                            class: "helper",
                                            "Current: Mic {device_label_for_id(&android_input_devices(), settings().android_input_device_id)} -> Output {device_label_for_id(&android_output_devices(), settings().android_output_device_id)}"
                                        }
                                    }
                                }
                                if let Some(last_saved_epoch) = selected_profile_last_saved() {
                                    p { class: "helper", "Last saved: {format_last_saved(last_saved_epoch)}" }
                                }
                                if !profile_feedback().is_empty() {
                                    p { class: "helper {profile_feedback_class()}", "{profile_feedback()}" }
                                }
                                if settings().profiles.is_empty() {
                                    p { class: "helper", "Save up to 3 profiles." }
                                }
                            }
                        }
                    }
                    }
                    p { class: "helper",
                        if !settings().profile_ready {
                            "Run the sweep once to unlock DSP."
                        } else if settings().dsp_enabled {
                            "DSP is active. Adjust EQ if needed."
                        } else {
                            "DSP is ready but muted."
                        }
                    }
                }

                section { class: if calibration_mode() { "panel sweep calibration-mode" } else { "panel sweep" },
                    div { class: "panel-header",
                        h2 { "Calibration sweep" }
                        span { class: "sweep-status",
                            if sweep().completed { "Completed" } else if sweep().running { "Running" } else { "Idle" }
                        }
                    }
                    p { class: "helper",
                        if sweep().completed {
                            "Sweep complete. The profile is now marked ready."
                        } else {
                            "Listen to the tone and tap what you hear. We adjust the loudness to find your threshold."
                        }
                    }
                    if !sweep().running && !sweep().completed {
                        button {
                            class: "primary",
                            onclick: move |_| {
                                calibration_mode.set(true);
                                sweep.with_mut(|state| {
                                    if state.completed {
                                        *state = BeepSweep::default();
                                    }
                                    state.running = true;
                                    state.step = 0;
                                });
                                settings.with_mut(|settings| {
                                    settings.sweep_results.clear();
                                    settings.thresholds.clear();
                                });
                                engine.with_mut(|engine| {
                                    engine.borrow_mut().apply(DspCommand::StartSweep);
                                });
                                (play_current_tone_for_calibration_start.borrow_mut())();
                            },
                            "Start calibration"
                        }
                    }
                    if calibration_mode() {
                        p { class: "helper", "Tap \"Heard it\" when you detect the tone." }
                    }
                    if let Some(level_db) = current_sweep_level() {
                        p { class: "helper", "Current level: {level_db:.1} dB" }
                    }
                    if show_advanced() && !settings().thresholds.is_empty() {
                        div { class: "thresholds",
                            h3 { "Your thresholds" }
                            div { class: "threshold-grid",
                                for threshold in settings.read().thresholds.iter() {
                                    div { class: "threshold-card",
                                        span { class: "band-label", "{threshold.frequency_hz} Hz" }
                                        span { class: "band-value", "{threshold.threshold_db:.1} dB" }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "sweep-actions",
                        button {
                            class: "secondary",
                            disabled: !sweep().running,
                            onclick: move |_| {
                                (play_current_tone_for_replay_button.borrow_mut())();
                            },
                            "Replay"
                        }
                        button {
                            class: "primary",
                            disabled: !sweep().running,
                            onclick: move |_| {
                                let handler = heard_response.clone();
                                (handler.borrow_mut())(true);
                            },
                            "Heard it"
                        }
                        button {
                            class: "ghost",
                            disabled: !sweep().running,
                            onclick: move |_| {
                                let handler = not_heard_response.clone();
                                (handler.borrow_mut())(false);
                            },
                            "Didn't hear"
                        }
                    }
                    div { class: "sweep-progress",
                        div {
                            class: "progress-bar",
                            style: "width: {sweep_progress}%;",
                        }
                    }
                    div { class: "sweep-steps",
                        for (index, band) in sweep.read().bands.iter().enumerate() {
                            div {
                                class: if index < sweep().step {
                                    "step done"
                                } else if index == sweep().step && sweep().running {
                                    "step active"
                                } else {
                                    "step"
                                },
                                "{band.frequency_hz} Hz"
                            }
                        }
                    }
                }

                section { class: "panel equalizer",
                    div { class: "panel-header",
                        h2 { "Hearing correction" }
                        div { class: "row",
                            if eq_editable() && has_user_eq_offsets() {
                                button {
                                    class: "ghost",
                                    onclick: move |_| {
                                        let updated_bands = settings.with_mut(|settings| {
                                            for band in settings.user_eq_offsets.iter_mut() {
                                                band.value = 0.0;
                                            }
                                            settings.eq_bands = combine_eq_layers(
                                                &settings.calibration_eq_bands,
                                                &settings.user_eq_offsets,
                                            );
                                            settings.eq_bands.clone()
                                        });
                                        engine.with_mut(|engine| {
                                            for (index, band) in updated_bands.iter().enumerate() {
                                                engine.borrow_mut().apply(DspCommand::SetEqBand {
                                                    index,
                                                    value_db: band.value,
                                                });
                                            }
                                            let (low, mid, high) = derive_band_gains(&updated_bands);
                                            engine.borrow_mut().apply(DspCommand::SetBandGains { low, mid, high });
                                        });
                                    },
                                    "Reset user EQ"
                                }
                            }
                            button {
                                class: if eq_editable() { "ghost active" } else { "ghost" },
                                onclick: move |_| {
                                    let next_value = !eq_editable();
                                    eq_editable.set(next_value);
                                    settings.with_mut(|settings| {
                                        settings.eq_editable = next_value;
                                    });
                                },
                                if eq_editable() { "Lock edits" } else { "Unlock edits" }
                            }
                        }
                    }
                    p { class: "helper",
                        if eq_editable() {
                            "Edit mode enabled. Changes are applied immediately."
                        } else {
                            "Read-only. Unlock edits to fine-tune gains."
                        }
                    }
                    if show_advanced() {
                        div { class: "eq-grid",
                        for (index, band) in settings.read().eq_bands.iter().enumerate() {
                            div { class: "eq-band",
                                span { class: "band-label", "{band.label}" }
                                input {
                                    r#type: "range",
                                    min: "-12",
                                    max: "12",
                                    step: "0.5",
                                    value: "{band.value}",
                                    disabled: !eq_editable(),
                                    oninput: move |event| {
                                        if let Ok(parsed) = event.value().parse::<f32>() {
                                            let clamped = parsed.clamp(-12.0, 12.0);
                                            settings.with_mut(|settings| {
                                                let calibration_value = settings
                                                    .calibration_eq_bands
                                                    .get(index)
                                                    .map(|band| band.value)
                                                    .unwrap_or(0.0);
                                                if let Some(user_band) = settings.user_eq_offsets.get_mut(index) {
                                                    user_band.value =
                                                        (clamped - calibration_value).clamp(-12.0, 12.0);
                                                }
                                                settings.eq_bands = combine_eq_layers(
                                                    &settings.calibration_eq_bands,
                                                    &settings.user_eq_offsets,
                                                );
                                                if let Some(band) = settings.eq_bands.get_mut(index) {
                                                    band.value = clamped;
                                                }
                                            });
                                            engine.with_mut(|engine| {
                                                engine.borrow_mut().apply(DspCommand::SetEqBand {
                                                    index,
                                                    value_db: clamped,
                                                });
                                                let bands = settings().eq_bands;
                                                let (low, mid, high) = derive_band_gains(&bands);
                                                engine.borrow_mut().apply(DspCommand::SetBandGains { low, mid, high });
                                            });
                                        }
                                    },
                                }
                                span { class: "band-value", "{band.value:.1} dB" }
                            }
                        }
                    }
                    } else {
                        div { class: "eq-grid compact",
                            for band in settings.read().eq_bands.iter() {
                                div { class: "eq-band",
                                    span { class: "band-label", "{band.label}" }
                                    span { class: "band-value", "{band.value:.1} dB" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn normalized_profile_name(raw: &str, index: usize) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        format!("Profile {}", index + 1)
    } else {
        trimmed.to_string()
    }
}

fn now_unix_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn format_last_saved(epoch_secs: u64) -> String {
    let absolute = format_epoch_utc(epoch_secs);

    let now = now_unix_epoch_secs();
    let elapsed = now.saturating_sub(epoch_secs);
    let relative = if elapsed < 60 {
        "just now".to_string()
    } else if elapsed < 3600 {
        format!("{}m ago", elapsed / 60)
    } else if elapsed < 86_400 {
        format!("{}h ago", elapsed / 3600)
    } else {
        format!("{}d ago", elapsed / 86_400)
    };

    format!("{absolute} ({relative})")
}

fn format_epoch_utc(epoch_secs: u64) -> String {
    let seconds = epoch_secs as i64;
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);

    let hour = (seconds_of_day / 3_600) as u32;
    let minute = ((seconds_of_day % 3_600) / 60) as u32;
    let second = (seconds_of_day % 60) as u32;

    // Civil date conversion adapted from Howard Hinnant's date algorithms.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        year, month, day, hour, minute, second
    )
}

fn profile_matches_settings(
    profile: &UserProfile,
    settings: &AppSettings,
    profile_name: &str,
    profile_index: usize,
) -> bool {
    profile.name == normalized_profile_name(profile_name, profile_index)
        && profile.profile_ready == settings.profile_ready
        && profile.eq_bands == settings.eq_bands
        && profile.calibration_eq_bands == settings.calibration_eq_bands
        && profile.user_eq_offsets == settings.user_eq_offsets
        && profile.noise_strength == settings.noise_strength
        && profile.noise_cancel == settings.noise_cancel
        && profile.noise_profile_mode == settings.noise_profile_mode
        && profile.limiter_enabled == settings.limiter_enabled
        && profile.safe_mode == settings.safe_mode
        && profile.master_gain == settings.master_gain
        && profile.input_gain == settings.input_gain
        && profile.agc_enabled == settings.agc_enabled
        && profile.agc_max_gain == settings.agc_max_gain
        && profile.android_backend == settings.android_backend
        && profile.android_burst == settings.android_burst
        && profile.android_input_device_id == settings.android_input_device_id
        && profile.android_output_device_id == settings.android_output_device_id
        && profile.low_cut_hz == settings.low_cut_hz
        && profile.high_cut_hz == settings.high_cut_hz
        && profile.thresholds == settings.thresholds
}

fn changed_fields_for_profile(
    profile: &UserProfile,
    settings: &AppSettings,
    profile_name: &str,
    profile_index: usize,
) -> Vec<&'static str> {
    let mut changed = Vec::new();
    if profile.name != normalized_profile_name(profile_name, profile_index) {
        changed.push("Name");
    }
    if profile.eq_bands != settings.eq_bands {
        changed.push("EQ");
    }
    if profile.calibration_eq_bands != settings.calibration_eq_bands {
        changed.push("Calibration EQ");
    }
    if profile.user_eq_offsets != settings.user_eq_offsets {
        changed.push("User EQ");
    }
    if profile.noise_cancel != settings.noise_cancel
        || profile.noise_strength != settings.noise_strength
        || profile.noise_profile_mode != settings.noise_profile_mode
    {
        changed.push("Noise");
    }
    if profile.limiter_enabled != settings.limiter_enabled
        || profile.safe_mode != settings.safe_mode
    {
        changed.push("Safety");
    }
    if profile.input_gain != settings.input_gain
        || profile.master_gain != settings.master_gain
        || profile.agc_enabled != settings.agc_enabled
        || profile.agc_max_gain != settings.agc_max_gain
    {
        changed.push("Gain");
    }
    if profile.android_backend != settings.android_backend
        || profile.android_burst != settings.android_burst
        || profile.android_input_device_id != settings.android_input_device_id
        || profile.android_output_device_id != settings.android_output_device_id
    {
        changed.push("Routing");
    }
    if profile.low_cut_hz != settings.low_cut_hz || profile.high_cut_hz != settings.high_cut_hz {
        changed.push("Tone");
    }
    if profile.thresholds != settings.thresholds {
        changed.push("Thresholds");
    }
    changed
}

fn device_label_for_id(devices: &[(i32, String)], device_id: i32) -> String {
    devices
        .iter()
        .find(|(id, _)| *id == device_id)
        .map(|(_, label)| label.clone())
        .unwrap_or_else(|| format!("Device {device_id}"))
}
