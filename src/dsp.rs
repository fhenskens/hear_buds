#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EngineState {
    Stopped,
    Running,
}

#[derive(Clone, PartialEq)]
pub struct SweepStep {
    pub frequency_hz: u32,
    pub amplitude: f32,
}

#[derive(Clone, PartialEq)]
pub enum DspCommand {
    Start,
    Stop,
    SetNoiseCancel(bool),
    SetNoiseStrength(f32),
    SetNoiseProfileMode(i32),
    CaptureNoiseProfile,
    SetLimiterEnabled(bool),
    SetBandGains {
        low: f32,
        mid: f32,
        high: f32,
    },
    SetSafeMode(bool),
    SetMasterGain(f32),
    SetInputGain(f32),
    SetAgcEnabled(bool),
    SetAgcMaxGain(f32),
    SetLowCutHz(f32),
    SetHighCutHz(f32),
    SetEqBand {
        index: usize,
        value_db: f32,
    },
    SetAndroidAudioConfig {
        backend: i32,
        frames_per_burst: u32,
    },
    SetAndroidPreferredDevices {
        input_device_id: i32,
        output_device_id: i32,
    },
    StartSweep,
    StopSweep,
    AdvanceSweep(SweepStep),
}

pub trait DspEngine {
    fn state(&self) -> EngineState;
    fn apply(&mut self, command: DspCommand);
}

#[cfg(target_os = "android")]
mod android;
#[cfg(all(
    not(target_os = "android"),
    not(target_arch = "wasm32"),
    feature = "desktop"
))]
mod desktop;

#[cfg(target_os = "android")]
pub use android::AndroidEngine as DefaultEngine;
#[cfg(target_os = "android")]
pub use android::AndroidEngine;

#[cfg(all(
    not(target_os = "android"),
    not(target_arch = "wasm32"),
    feature = "desktop"
))]
pub use desktop::DesktopEngine as DefaultEngine;

#[cfg(any(
    target_arch = "wasm32",
    all(
        not(target_os = "android"),
        not(target_arch = "wasm32"),
        not(feature = "desktop")
    )
))]
pub type DefaultEngine = NoopEngine;

/// Placeholder engine for wasm builds.
#[cfg(any(
    target_arch = "wasm32",
    all(
        not(target_os = "android"),
        not(target_arch = "wasm32"),
        not(feature = "desktop")
    )
))]
pub struct NoopEngine {
    state: EngineState,
}

#[cfg(any(
    target_arch = "wasm32",
    all(
        not(target_os = "android"),
        not(target_arch = "wasm32"),
        not(feature = "desktop")
    )
))]
impl NoopEngine {
    pub fn new() -> Self {
        Self {
            state: EngineState::Stopped,
        }
    }
}

#[cfg(any(
    target_arch = "wasm32",
    all(
        not(target_os = "android"),
        not(target_arch = "wasm32"),
        not(feature = "desktop")
    )
))]
impl DspEngine for NoopEngine {
    fn state(&self) -> EngineState {
        self.state
    }

    fn apply(&mut self, command: DspCommand) {
        match command {
            DspCommand::Start => self.state = EngineState::Running,
            DspCommand::Stop => self.state = EngineState::Stopped,
            DspCommand::SetNoiseCancel(_) => {}
            DspCommand::SetNoiseStrength(_) => {}
            DspCommand::SetNoiseProfileMode(_) => {}
            DspCommand::CaptureNoiseProfile => {}
            DspCommand::SetLimiterEnabled(_) => {}
            DspCommand::SetBandGains { .. } => {}
            DspCommand::SetSafeMode(_) => {}
            DspCommand::SetMasterGain(_) => {}
            DspCommand::SetInputGain(_) => {}
            DspCommand::SetAgcEnabled(_) => {}
            DspCommand::SetAgcMaxGain(_) => {}
            DspCommand::SetLowCutHz(_) => {}
            DspCommand::SetHighCutHz(_) => {}
            DspCommand::SetEqBand { .. } => {}
            DspCommand::SetAndroidAudioConfig { .. } => {}
            DspCommand::SetAndroidPreferredDevices { .. } => {}
            DspCommand::StartSweep => {}
            DspCommand::StopSweep => {}
            DspCommand::AdvanceSweep(_) => {}
        }
    }
}

#[cfg(any(
    target_arch = "wasm32",
    all(
        not(target_os = "android"),
        not(target_arch = "wasm32"),
        not(feature = "desktop")
    )
))]
impl NoopEngine {
    pub fn callback_ms(&self) -> f32 {
        0.0
    }

    pub fn input_peak(&self) -> f32 {
        0.0
    }

    pub fn underruns(&self) -> u64 {
        0
    }

    pub fn trimmed_samples(&self) -> u64 {
        0
    }

    pub fn clipped_samples(&self) -> u64 {
        0
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
