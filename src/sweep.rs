use crate::state::{EqBand, HearingThreshold, SweepResult};

pub fn derive_eq_from_sweep(
    bands: &[EqBand],
    results: &[SweepResult],
    thresholds: &[HearingThreshold],
) -> Vec<EqBand> {
    let mut points: Vec<(f64, f64)> = Vec::new();

    if !thresholds.is_empty() {
        let reference_db = -30.0;
        for threshold in thresholds {
            let loss = (threshold.threshold_db - reference_db).max(0.0);
            let gain = (loss * 0.6).clamp(0.0, 15.0);
            points.push((threshold.frequency_hz as f64, gain as f64));
        }
    } else {
        points.extend(results.iter().map(|result| {
            let target_db = if result.heard { 0.0 } else { 6.0 };
            (result.frequency_hz as f64, target_db)
        }));
    }

    points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    points.dedup_by(|a, b| a.0 == b.0);

    if points.len() < 2 {
        return bands.to_vec();
    }

    let linear = LinearCurve::new(&points);
    let mut derived: Vec<EqBand> = bands
        .iter()
        .map(|band| {
            let freq = band.frequency_hz as f64;
            let value = linear.sample(freq).clamp(-12.0, 12.0) as f32;
            EqBand {
                label: band.label.clone(),
                frequency_hz: band.frequency_hz,
                value,
            }
        })
        .collect();

    apply_smoothing(&mut derived, 0.25);
    derived
}

pub fn derive_band_gains(eq_bands: &[EqBand]) -> (f32, f32, f32) {
    let mut low = Vec::new();
    let mut mid = Vec::new();
    let mut high = Vec::new();

    for band in eq_bands {
        let freq = band.frequency_hz;
        if freq <= 500 {
            low.push(band.value);
        } else if freq <= 2000 {
            mid.push(band.value);
        } else {
            high.push(band.value);
        }
    }

    let avg = |values: &[f32]| {
        if values.is_empty() {
            0.0
        } else {
            values.iter().sum::<f32>() / values.len() as f32
        }
    };

    let low_gain = db_to_gain(avg(&low));
    let mid_gain = db_to_gain(avg(&mid));
    let high_gain = db_to_gain(avg(&high));

    (low_gain, mid_gain, high_gain)
}

fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0).clamp(0.5, 3.0)
}

struct LinearCurve {
    xs: Vec<f64>,
    ys: Vec<f64>,
}

impl LinearCurve {
    fn new(points: &[(f64, f64)]) -> Self {
        let mut xs = Vec::with_capacity(points.len());
        let mut ys = Vec::with_capacity(points.len());
        for (x, y) in points {
            xs.push(*x);
            ys.push(*y);
        }
        Self { xs, ys }
    }

    fn sample(&self, x: f64) -> f64 {
        let n = self.xs.len();
        if x <= self.xs[0] {
            return self.ys[0];
        }
        if x >= self.xs[n - 1] {
            return self.ys[n - 1];
        }

        let mut low = 0usize;
        let mut high = n - 1;
        while high - low > 1 {
            let mid = (high + low) / 2;
            if self.xs[mid] > x {
                high = mid;
            } else {
                low = mid;
            }
        }

        let h = self.xs[high] - self.xs[low];
        if h == 0.0 {
            return self.ys[low];
        }

        let t = (x - self.xs[low]) / h;
        self.ys[low] + t * (self.ys[high] - self.ys[low])
    }
}

fn apply_smoothing(bands: &mut [EqBand], alpha: f32) {
    if bands.len() < 2 {
        return;
    }

    let mut previous = bands[0].value;
    for band in bands.iter_mut().skip(1) {
        let smoothed = previous + alpha * (band.value - previous);
        band.value = smoothed;
        previous = smoothed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_bands() -> Vec<EqBand> {
        vec![
            EqBand {
                label: "125 Hz".to_string(),
                frequency_hz: 125,
                value: 0.0,
            },
            EqBand {
                label: "250 Hz".to_string(),
                frequency_hz: 250,
                value: 0.0,
            },
            EqBand {
                label: "500 Hz".to_string(),
                frequency_hz: 500,
                value: 0.0,
            },
            EqBand {
                label: "1 kHz".to_string(),
                frequency_hz: 1000,
                value: 0.0,
            },
            EqBand {
                label: "2 kHz".to_string(),
                frequency_hz: 2000,
                value: 0.0,
            },
            EqBand {
                label: "4 kHz".to_string(),
                frequency_hz: 4000,
                value: 0.0,
            },
            EqBand {
                label: "8 kHz".to_string(),
                frequency_hz: 8000,
                value: 0.0,
            },
        ]
    }

    #[test]
    fn derive_eq_returns_original_when_not_enough_points() {
        let bands = test_bands();
        let results = vec![SweepResult {
            frequency_hz: 1000,
            level_db: -20.0,
            heard: true,
        }];
        let derived = derive_eq_from_sweep(&bands, &results, &[]);
        assert_eq!(derived.len(), bands.len());
        assert!(derived == bands);
    }

    #[test]
    fn thresholds_map_to_non_negative_gains() {
        let bands = test_bands();
        let thresholds = vec![
            HearingThreshold {
                frequency_hz: 125,
                threshold_db: -10.0,
            },
            HearingThreshold {
                frequency_hz: 8000,
                threshold_db: -5.0,
            },
        ];
        let derived = derive_eq_from_sweep(&bands, &[], &thresholds);
        assert!(derived.iter().all(|band| band.value >= 0.0));
    }

    #[test]
    fn derived_eq_values_are_clamped() {
        let bands = test_bands();
        let thresholds = vec![
            HearingThreshold {
                frequency_hz: 125,
                threshold_db: 80.0,
            },
            HearingThreshold {
                frequency_hz: 8000,
                threshold_db: 120.0,
            },
        ];
        let derived = derive_eq_from_sweep(&bands, &[], &thresholds);
        assert!(derived
            .iter()
            .all(|band| (-12.0..=12.0).contains(&band.value)));
    }

    #[test]
    fn smoothing_reduces_adjacent_jumps() {
        let bands = test_bands();
        let thresholds = vec![
            HearingThreshold {
                frequency_hz: 125,
                threshold_db: -30.0,
            },
            HearingThreshold {
                frequency_hz: 8000,
                threshold_db: 40.0,
            },
        ];
        let derived = derive_eq_from_sweep(&bands, &[], &thresholds);

        for window in derived.windows(2) {
            let jump = (window[1].value - window[0].value).abs();
            assert!(jump < 12.0);
        }
    }

    #[test]
    fn derive_band_gains_uses_expected_buckets() {
        let eq = vec![
            EqBand {
                label: "125 Hz".to_string(),
                frequency_hz: 125,
                value: 6.0,
            },
            EqBand {
                label: "250 Hz".to_string(),
                frequency_hz: 250,
                value: 6.0,
            },
            EqBand {
                label: "500 Hz".to_string(),
                frequency_hz: 500,
                value: 6.0,
            },
            EqBand {
                label: "1 kHz".to_string(),
                frequency_hz: 1000,
                value: 0.0,
            },
            EqBand {
                label: "2 kHz".to_string(),
                frequency_hz: 2000,
                value: 0.0,
            },
            EqBand {
                label: "4 kHz".to_string(),
                frequency_hz: 4000,
                value: -6.0,
            },
            EqBand {
                label: "8 kHz".to_string(),
                frequency_hz: 8000,
                value: -6.0,
            },
        ];
        let (low, mid, high) = derive_band_gains(&eq);
        assert!(low > mid);
        assert!(mid > high);
    }

    #[test]
    fn results_only_path_derives_higher_gain_for_not_heard() {
        let bands = test_bands();
        let results = vec![
            SweepResult {
                frequency_hz: 125,
                level_db: -20.0,
                heard: false,
            },
            SweepResult {
                frequency_hz: 8000,
                level_db: -20.0,
                heard: true,
            },
        ];

        let derived = derive_eq_from_sweep(&bands, &results, &[]);
        let low = derived
            .iter()
            .find(|band| band.frequency_hz == 125)
            .map(|band| band.value)
            .unwrap_or_default();
        let high = derived
            .iter()
            .find(|band| band.frequency_hz == 8000)
            .map(|band| band.value)
            .unwrap_or_default();
        assert!(low > high);
    }

    #[test]
    fn derive_band_gains_clamps_extreme_values() {
        let eq = vec![
            EqBand {
                label: "125 Hz".to_string(),
                frequency_hz: 125,
                value: 80.0,
            },
            EqBand {
                label: "500 Hz".to_string(),
                frequency_hz: 500,
                value: 80.0,
            },
            EqBand {
                label: "1 kHz".to_string(),
                frequency_hz: 1000,
                value: 80.0,
            },
            EqBand {
                label: "8 kHz".to_string(),
                frequency_hz: 8000,
                value: -80.0,
            },
        ];
        let (low, mid, high) = derive_band_gains(&eq);
        assert!((low - 3.0).abs() < 1.0e-6);
        assert!((mid - 3.0).abs() < 1.0e-6);
        assert!((high - 0.5).abs() < 1.0e-6);
    }
}
