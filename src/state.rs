use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const SETTINGS_FILE: &str = "hear_buds_settings.json";

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct EqBand {
    pub label: String,
    #[serde(default)]
    pub frequency_hz: u32,
    pub value: f32,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    pub dsp_enabled: bool,
    #[serde(default)]
    pub show_advanced: bool,
    #[serde(default)]
    pub eq_editable: bool,
    #[serde(default = "default_selected_profile_index")]
    pub selected_profile_index: usize,
    #[serde(default = "default_selected_profile_name")]
    pub selected_profile_name: String,
    pub profile_ready: bool,
    pub noise_cancel: bool,
    pub noise_strength: f32,
    #[serde(default = "default_noise_profile_mode")]
    pub noise_profile_mode: i32,
    pub limiter_enabled: bool,
    pub safe_mode: bool,
    #[serde(default = "default_master_gain")]
    pub master_gain: f32,
    #[serde(default = "default_input_gain")]
    pub input_gain: f32,
    #[serde(default = "default_agc_enabled")]
    pub agc_enabled: bool,
    #[serde(default = "default_agc_max_gain")]
    pub agc_max_gain: f32,
    #[serde(default = "default_android_backend")]
    pub android_backend: i32,
    #[serde(default = "default_android_burst")]
    pub android_burst: u32,
    #[serde(default = "default_android_input_device_id")]
    pub android_input_device_id: i32,
    #[serde(default = "default_android_output_device_id")]
    pub android_output_device_id: i32,
    #[serde(default)]
    pub calibration_eq_bands: Vec<EqBand>,
    #[serde(default)]
    pub user_eq_offsets: Vec<EqBand>,
    #[serde(default = "default_low_cut_hz")]
    pub low_cut_hz: f32,
    #[serde(default = "default_high_cut_hz")]
    pub high_cut_hz: f32,
    pub eq_bands: Vec<EqBand>,
    pub sweep_results: Vec<SweepResult>,
    #[serde(default)]
    pub thresholds: Vec<HearingThreshold>,
    #[serde(default)]
    pub profiles: Vec<UserProfile>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct SweepResult {
    pub frequency_hz: u32,
    #[serde(default)]
    pub level_db: f32,
    pub heard: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct HearingThreshold {
    pub frequency_hz: u32,
    pub threshold_db: f32,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct UserProfile {
    pub name: String,
    #[serde(default = "default_last_saved_epoch_secs")]
    pub last_saved_epoch_secs: u64,
    #[serde(default)]
    pub profile_ready: bool,
    pub eq_bands: Vec<EqBand>,
    pub noise_strength: f32,
    pub noise_cancel: bool,
    #[serde(default = "default_noise_profile_mode")]
    pub noise_profile_mode: i32,
    pub limiter_enabled: bool,
    pub safe_mode: bool,
    #[serde(default = "default_master_gain")]
    pub master_gain: f32,
    #[serde(default = "default_input_gain")]
    pub input_gain: f32,
    #[serde(default = "default_agc_enabled")]
    pub agc_enabled: bool,
    #[serde(default = "default_agc_max_gain")]
    pub agc_max_gain: f32,
    #[serde(default = "default_android_backend")]
    pub android_backend: i32,
    #[serde(default = "default_android_burst")]
    pub android_burst: u32,
    #[serde(default = "default_android_input_device_id")]
    pub android_input_device_id: i32,
    #[serde(default = "default_android_output_device_id")]
    pub android_output_device_id: i32,
    #[serde(default)]
    pub calibration_eq_bands: Vec<EqBand>,
    #[serde(default)]
    pub user_eq_offsets: Vec<EqBand>,
    #[serde(default = "default_low_cut_hz")]
    pub low_cut_hz: f32,
    #[serde(default = "default_high_cut_hz")]
    pub high_cut_hz: f32,
    pub thresholds: Vec<HearingThreshold>,
}

impl Default for AppSettings {
    fn default() -> Self {
        let calibration_eq_bands = default_calibration_eq_bands();
        let user_eq_offsets = default_user_eq_offsets();
        Self {
            dsp_enabled: false,
            show_advanced: false,
            eq_editable: false,
            selected_profile_index: default_selected_profile_index(),
            selected_profile_name: default_selected_profile_name(),
            profile_ready: false,
            noise_cancel: false,
            noise_strength: 0.6,
            noise_profile_mode: default_noise_profile_mode(),
            limiter_enabled: true,
            safe_mode: true,
            master_gain: default_master_gain(),
            input_gain: default_input_gain(),
            agc_enabled: default_agc_enabled(),
            agc_max_gain: default_agc_max_gain(),
            android_backend: default_android_backend(),
            android_burst: default_android_burst(),
            android_input_device_id: default_android_input_device_id(),
            android_output_device_id: default_android_output_device_id(),
            calibration_eq_bands: calibration_eq_bands.clone(),
            user_eq_offsets: user_eq_offsets.clone(),
            low_cut_hz: default_low_cut_hz(),
            high_cut_hz: default_high_cut_hz(),
            eq_bands: combine_eq_layers(&calibration_eq_bands, &user_eq_offsets),
            sweep_results: Vec::new(),
            thresholds: Vec::new(),
            profiles: Vec::new(),
        }
    }
}

fn default_master_gain() -> f32 {
    1.25
}

fn default_input_gain() -> f32 {
    1.0
}

fn default_agc_enabled() -> bool {
    true
}

fn default_agc_max_gain() -> f32 {
    1.0
}

fn default_noise_profile_mode() -> i32 {
    0
}

fn default_android_backend() -> i32 {
    0
}

fn default_android_burst() -> u32 {
    192
}

fn default_android_input_device_id() -> i32 {
    0
}

fn default_android_output_device_id() -> i32 {
    0
}

fn default_low_cut_hz() -> f32 {
    20.0
}

fn default_high_cut_hz() -> f32 {
    6600.0
}

fn default_calibration_eq_bands() -> Vec<EqBand> {
    vec![
        EqBand {
            label: "125 Hz".to_string(),
            frequency_hz: 125,
            value: 2.0,
        },
        EqBand {
            label: "250 Hz".to_string(),
            frequency_hz: 250,
            value: 3.5,
        },
        EqBand {
            label: "500 Hz".to_string(),
            frequency_hz: 500,
            value: 1.0,
        },
        EqBand {
            label: "1 kHz".to_string(),
            frequency_hz: 1000,
            value: 0.0,
        },
        EqBand {
            label: "2 kHz".to_string(),
            frequency_hz: 2000,
            value: -1.5,
        },
        EqBand {
            label: "4 kHz".to_string(),
            frequency_hz: 4000,
            value: -2.5,
        },
        EqBand {
            label: "8 kHz".to_string(),
            frequency_hz: 8000,
            value: -3.0,
        },
    ]
}

fn default_user_eq_offsets() -> Vec<EqBand> {
    default_calibration_eq_bands()
        .into_iter()
        .map(|mut band| {
            band.value = 0.0;
            band
        })
        .collect()
}

fn default_last_saved_epoch_secs() -> u64 {
    0
}

fn default_selected_profile_index() -> usize {
    0
}

fn default_selected_profile_name() -> String {
    "Profile 1".to_string()
}

pub fn combine_eq_layers(
    calibration_eq_bands: &[EqBand],
    user_eq_offsets: &[EqBand],
) -> Vec<EqBand> {
    calibration_eq_bands
        .iter()
        .enumerate()
        .map(|(index, calibration)| {
            let user = user_eq_offsets
                .get(index)
                .map(|band| band.value)
                .unwrap_or(0.0);
            EqBand {
                label: calibration.label.clone(),
                frequency_hz: calibration.frequency_hz,
                value: (calibration.value + user).clamp(-12.0, 12.0),
            }
        })
        .collect()
}

pub fn load_settings() -> Option<AppSettings> {
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = settings_path()?;
        let contents = std::fs::read_to_string(path).ok()?;
        let mut settings: AppSettings = serde_json::from_str(&contents).ok()?;
        normalize_settings(&mut settings);
        settings.dsp_enabled = false;
        Some(settings)
    }
}

pub fn save_settings(settings: &AppSettings) {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = settings;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(path) = settings_path() else {
            return;
        };

        let mut persisted = settings.clone();
        persisted.dsp_enabled = false;

        if let Ok(contents) = serde_json::to_string_pretty(&persisted) {
            let _ = std::fs::write(path, contents);
        }
    }
}

fn normalize_settings(settings: &mut AppSettings) {
    settings.selected_profile_index = settings.selected_profile_index.min(2);
    if settings.selected_profile_name.trim().is_empty() {
        settings.selected_profile_name = default_selected_profile_name();
    }

    normalize_eq_bands(&mut settings.eq_bands);
    normalize_eq_bands(&mut settings.calibration_eq_bands);
    normalize_eq_bands(&mut settings.user_eq_offsets);

    settings.noise_strength = settings.noise_strength.clamp(0.0, 1.0);
    settings.noise_profile_mode = settings.noise_profile_mode.clamp(0, 1);
    settings.master_gain = settings.master_gain.clamp(0.5, 6.0);
    settings.input_gain = settings.input_gain.clamp(0.5, 20.0);
    settings.agc_max_gain = settings.agc_max_gain.clamp(1.0, 20.0);
    settings.android_backend = settings.android_backend.clamp(0, 2);
    settings.android_burst = settings.android_burst.clamp(64, 512);
    settings.android_input_device_id = settings.android_input_device_id.max(0);
    settings.android_output_device_id = settings.android_output_device_id.max(0);
    settings.low_cut_hz = settings.low_cut_hz.clamp(20.0, 400.0);
    settings.high_cut_hz = settings.high_cut_hz.clamp(2000.0, 12_000.0);

    if settings.calibration_eq_bands.is_empty() {
        settings.calibration_eq_bands = settings.eq_bands.clone();
    }
    if settings.user_eq_offsets.is_empty() {
        settings.user_eq_offsets = settings
            .calibration_eq_bands
            .iter()
            .map(|band| EqBand {
                label: band.label.clone(),
                frequency_hz: band.frequency_hz,
                value: 0.0,
            })
            .collect();
    }
    if settings.calibration_eq_bands.len() != settings.user_eq_offsets.len() {
        settings.user_eq_offsets = settings
            .calibration_eq_bands
            .iter()
            .map(|band| EqBand {
                label: band.label.clone(),
                frequency_hz: band.frequency_hz,
                value: 0.0,
            })
            .collect();
    }
    settings.eq_bands =
        combine_eq_layers(&settings.calibration_eq_bands, &settings.user_eq_offsets);

    if settings.profiles.len() > 3 {
        settings.profiles.truncate(3);
    }
    for profile in &mut settings.profiles {
        normalize_profile(profile);
    }
}

fn normalize_eq_bands(eq_bands: &mut [EqBand]) {
    for band in eq_bands {
        if band.frequency_hz == 0 {
            band.frequency_hz = parse_frequency_hz(&band.label).unwrap_or(0);
        }
        band.value = band.value.clamp(-12.0, 12.0);
    }
}

fn normalize_profile(profile: &mut UserProfile) {
    if profile.name.trim().is_empty() {
        profile.name = "Profile".to_string();
    }
    normalize_eq_bands(&mut profile.eq_bands);
    normalize_eq_bands(&mut profile.calibration_eq_bands);
    normalize_eq_bands(&mut profile.user_eq_offsets);
    profile.noise_strength = profile.noise_strength.clamp(0.0, 1.0);
    profile.noise_profile_mode = profile.noise_profile_mode.clamp(0, 1);
    profile.master_gain = profile.master_gain.clamp(0.5, 6.0);
    profile.input_gain = profile.input_gain.clamp(0.5, 20.0);
    profile.agc_max_gain = profile.agc_max_gain.clamp(1.0, 20.0);
    profile.android_backend = profile.android_backend.clamp(0, 2);
    profile.android_burst = profile.android_burst.clamp(64, 512);
    profile.android_input_device_id = profile.android_input_device_id.max(0);
    profile.android_output_device_id = profile.android_output_device_id.max(0);
    profile.low_cut_hz = profile.low_cut_hz.clamp(20.0, 400.0);
    profile.high_cut_hz = profile.high_cut_hz.clamp(2000.0, 12_000.0);
    if profile.calibration_eq_bands.is_empty() {
        profile.calibration_eq_bands = profile.eq_bands.clone();
    }
    if profile.user_eq_offsets.is_empty()
        || profile.user_eq_offsets.len() != profile.calibration_eq_bands.len()
    {
        profile.user_eq_offsets = profile
            .calibration_eq_bands
            .iter()
            .map(|band| EqBand {
                label: band.label.clone(),
                frequency_hz: band.frequency_hz,
                value: 0.0,
            })
            .collect();
    }
    profile.eq_bands = combine_eq_layers(&profile.calibration_eq_bands, &profile.user_eq_offsets);
}

fn parse_frequency_hz(label: &str) -> Option<u32> {
    let trimmed = label.trim().to_lowercase();
    if let Some(khz) = trimmed.strip_suffix("khz") {
        let value: f32 = khz.trim().parse().ok()?;
        return Some((value * 1000.0).round() as u32);
    }
    if let Some(hz) = trimmed.strip_suffix("hz") {
        let value: f32 = hz.trim().parse().ok()?;
        return Some(value.round() as u32);
    }
    None
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
fn settings_path() -> Option<PathBuf> {
    let mut path = std::env::current_dir().ok()?;
    path.push(SETTINGS_FILE);
    Some(path)
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
fn settings_path() -> Option<PathBuf> {
    use jni::objects::{JObject, JString};
    use jni::sys::jobject;
    use jni::JavaVM;

    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let context = unsafe { JObject::from_raw(ctx.context() as jobject) };

    let files_dir = env
        .call_method(&context, "getFilesDir", "()Ljava/io/File;", &[])
        .ok()?
        .l()
        .ok()?;

    let path_obj = env
        .call_method(files_dir, "getAbsolutePath", "()Ljava/lang/String;", &[])
        .ok()?
        .l()
        .ok()?;

    let path_jstring: JString = JString::from(path_obj);
    let path: String = env.get_string(&path_jstring).ok()?.into();
    let mut buf = PathBuf::from(path);
    buf.push(SETTINGS_FILE);
    Some(buf)
}

#[cfg(target_arch = "wasm32")]
fn settings_path() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_frequency_supports_hz_and_khz() {
        assert_eq!(parse_frequency_hz("125 Hz"), Some(125));
        assert_eq!(parse_frequency_hz("1 kHz"), Some(1000));
        assert_eq!(parse_frequency_hz(" 2.5khz "), Some(2500));
        assert_eq!(parse_frequency_hz("n/a"), None);
    }

    #[test]
    fn normalize_settings_fills_missing_band_frequency() {
        let mut settings = AppSettings::default();
        settings.eq_bands = vec![
            EqBand {
                label: "1 kHz".to_string(),
                frequency_hz: 0,
                value: 0.0,
            },
            EqBand {
                label: "custom".to_string(),
                frequency_hz: 0,
                value: 0.0,
            },
        ];
        settings.calibration_eq_bands = settings.eq_bands.clone();
        settings.user_eq_offsets = settings
            .eq_bands
            .iter()
            .map(|band| EqBand {
                label: band.label.clone(),
                frequency_hz: band.frequency_hz,
                value: 0.0,
            })
            .collect();
        normalize_settings(&mut settings);
        assert_eq!(settings.eq_bands[0].frequency_hz, 1000);
        assert_eq!(settings.eq_bands[1].frequency_hz, 0);
    }

    #[test]
    fn deserialization_applies_defaults_for_new_fields() {
        let json = r#"{
            "dsp_enabled": false,
            "profile_ready": false,
            "noise_cancel": false,
            "noise_strength": 0.6,
            "limiter_enabled": true,
            "safe_mode": true,
            "eq_bands": [
                { "label": "1 kHz", "frequency_hz": 1000, "value": 0.0 }
            ],
            "sweep_results": [],
            "thresholds": [],
            "profiles": []
        }"#;

        let parsed: AppSettings =
            serde_json::from_str(json).expect("AppSettings JSON should parse");
        assert!((parsed.master_gain - 1.25).abs() < 1.0e-6);
        assert!((parsed.input_gain - 1.0).abs() < 1.0e-6);
        assert!(parsed.agc_enabled);
        assert!((parsed.agc_max_gain - 1.0).abs() < 1.0e-6);
        assert_eq!(parsed.android_backend, 0);
        assert_eq!(parsed.android_burst, 192);
        assert_eq!(parsed.android_input_device_id, 0);
        assert_eq!(parsed.android_output_device_id, 0);
        assert!((parsed.low_cut_hz - 20.0).abs() < 1.0e-6);
        assert!((parsed.high_cut_hz - 6600.0).abs() < 1.0e-6);
        let profile_json = r#"{
            "name": "Test",
            "profile_ready": true,
            "eq_bands": [],
            "noise_strength": 0.5,
            "noise_cancel": false,
            "limiter_enabled": true,
            "safe_mode": true,
            "thresholds": []
        }"#;
        let parsed_profile: UserProfile =
            serde_json::from_str(profile_json).expect("UserProfile JSON should parse");
        assert_eq!(parsed_profile.last_saved_epoch_secs, 0);
    }

    #[test]
    fn normalize_settings_clamps_fields_and_profiles() {
        let mut settings = AppSettings::default();
        settings.noise_strength = 3.0;
        settings.master_gain = -1.0;
        settings.input_gain = 99.0;
        settings.agc_max_gain = 99.0;
        settings.android_backend = 99;
        settings.android_burst = 1;
        settings.android_input_device_id = -5;
        settings.android_output_device_id = -7;
        settings.low_cut_hz = -1.0;
        settings.high_cut_hz = 30_000.0;
        settings.calibration_eq_bands[0].value = 50.0;
        settings.eq_bands[0].value = 50.0;
        settings.profiles.push(UserProfile {
            name: "".to_string(),
            last_saved_epoch_secs: 0,
            profile_ready: false,
            eq_bands: vec![EqBand {
                label: "1 kHz".to_string(),
                frequency_hz: 0,
                value: -99.0,
            }],
            noise_strength: -1.0,
            noise_cancel: false,
            limiter_enabled: true,
            safe_mode: true,
            master_gain: 99.0,
            input_gain: -5.0,
            agc_enabled: true,
            agc_max_gain: -2.0,
            android_backend: -3,
            android_burst: 9,
            android_input_device_id: -1,
            android_output_device_id: -1,
            calibration_eq_bands: Vec::new(),
            user_eq_offsets: Vec::new(),
            low_cut_hz: -10.0,
            high_cut_hz: 99_999.0,
            thresholds: Vec::new(),
        });

        normalize_settings(&mut settings);

        assert!((settings.noise_strength - 1.0).abs() < 1.0e-6);
        assert!((settings.master_gain - 0.5).abs() < 1.0e-6);
        assert!((settings.input_gain - 20.0).abs() < 1.0e-6);
        assert!((settings.agc_max_gain - 20.0).abs() < 1.0e-6);
        assert_eq!(settings.android_backend, 2);
        assert_eq!(settings.android_burst, 64);
        assert_eq!(settings.android_input_device_id, 0);
        assert_eq!(settings.android_output_device_id, 0);
        assert!((settings.low_cut_hz - 20.0).abs() < 1.0e-6);
        assert!((settings.high_cut_hz - 12_000.0).abs() < 1.0e-6);
        assert!((settings.eq_bands[0].value - 12.0).abs() < 1.0e-6);
        assert_eq!(settings.profiles[0].name, "Profile");
        assert_eq!(settings.profiles[0].eq_bands[0].frequency_hz, 1000);
        assert!((settings.profiles[0].eq_bands[0].value + 12.0).abs() < 1.0e-6);
    }
}
