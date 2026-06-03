//! # ruvos-stream — inflight stream analysis
//!
//! Pure-`std` primitives for analyzing an agent's output *as it streams*, vendored
//! from rUvnet's **midstream** (`midstreamer-temporal-compare`, © Reuven Cohen /
//! @ruvnet, MIT):
//!
//! - [`dtw_distance`] — Dynamic Time Warping distance between two numeric
//!   trajectories, for comparing a run's shape against a reference.
//! - [`DriftMonitor`] — an online (single-pass) anomaly detector: feed it a value
//!   per chunk and it flags z-score outliers once it has seen enough samples.
//!
//! rUvOS feeds the [`DriftMonitor`] the size of each streamed output chunk from an
//! agent runner, surfacing a live "is this run behaving normally?" signal instead
//! of only judging the final result.

/// Dynamic Time Warping distance between two sequences (absolute-difference cost).
/// `0.0` for identical sequences; tolerant of stretching/compression in time.
pub fn dtw_distance(a: &[f64], b: &[f64]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    if a.is_empty() || b.is_empty() {
        return f64::INFINITY;
    }
    let (n, m) = (a.len(), b.len());
    let mut prev = vec![f64::INFINITY; m + 1];
    let mut cur = vec![f64::INFINITY; m + 1];
    prev[0] = 0.0; // dtw[0][0]

    for i in 1..=n {
        cur[0] = f64::INFINITY;
        for j in 1..=m {
            let cost = (a[i - 1] - b[j - 1]).abs();
            let best = prev[j].min(cur[j - 1]).min(prev[j - 1]);
            cur[j] = cost + best;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[m]
}

/// Number of samples observed before the monitor will flag anomalies (warm-up).
const WARMUP: u64 = 8;

/// Online drift / anomaly detector over a stream of scalar observations.
///
/// Maintains a running mean and variance (Welford's algorithm, single pass, O(1)
/// memory) and flags a value as anomalous when its z-score exceeds `threshold`
/// (after `WARMUP` samples). Anomaly detection uses the stats *before* the value is
/// folded in, so a sudden spike is caught rather than masked.
#[derive(Debug, Clone)]
pub struct DriftMonitor {
    n: u64,
    mean: f64,
    m2: f64,
    anomalies: u64,
    threshold: f64,
}

impl DriftMonitor {
    /// `threshold` is the z-score above which a value is anomalous (e.g. `3.0`).
    pub fn new(threshold: f64) -> Self {
        Self {
            n: 0,
            mean: 0.0,
            m2: 0.0,
            anomalies: 0,
            threshold,
        }
    }

    /// Observe one value; returns `true` if it is anomalous vs. the stream so far.
    pub fn observe(&mut self, x: f64) -> bool {
        let anomaly = if self.n >= WARMUP {
            let std = (self.m2 / self.n as f64).sqrt();
            if std > 0.0 {
                ((x - self.mean).abs() / std) > self.threshold
            } else {
                // A perfectly stable baseline has zero variance; any deviation
                // from it is itself drift (z-score would be infinite).
                (x - self.mean).abs() > f64::EPSILON
            }
        } else {
            false
        };
        // Welford update.
        self.n += 1;
        let delta = x - self.mean;
        self.mean += delta / self.n as f64;
        let delta2 = x - self.mean;
        self.m2 += delta * delta2;
        if anomaly {
            self.anomalies += 1;
        }
        anomaly
    }

    pub fn count(&self) -> u64 {
        self.n
    }
    pub fn anomalies(&self) -> u64 {
        self.anomalies
    }
    pub fn mean(&self) -> f64 {
        self.mean
    }
}

impl Default for DriftMonitor {
    fn default() -> Self {
        Self::new(3.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dtw_identical_is_zero() {
        let a = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(dtw_distance(&a, &a), 0.0);
    }

    #[test]
    fn dtw_time_warp_is_small() {
        // Same shape, stretched in time → small distance.
        let a = [0.0, 1.0, 2.0, 1.0, 0.0];
        let b = [0.0, 1.0, 1.0, 2.0, 1.0, 0.0];
        let warped = dtw_distance(&a, &b);
        let unrelated = dtw_distance(&a, &[5.0, 5.0, 5.0, 5.0, 5.0]);
        assert!(
            warped < unrelated,
            "warped {warped} should beat unrelated {unrelated}"
        );
    }

    #[test]
    fn dtw_empty_edges() {
        assert_eq!(dtw_distance(&[], &[]), 0.0);
        assert_eq!(dtw_distance(&[1.0], &[]), f64::INFINITY);
    }

    #[test]
    fn drift_stable_stream_has_no_anomalies() {
        let mut m = DriftMonitor::new(3.0);
        for _ in 0..50 {
            assert!(!m.observe(10.0));
        }
        assert_eq!(m.anomalies(), 0);
        assert!((m.mean() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn drift_flags_outlier_after_warmup() {
        let mut m = DriftMonitor::new(3.0);
        // Establish a tight baseline with mild variation.
        for i in 0..20 {
            let x = 10.0 + if i % 2 == 0 { 0.5 } else { -0.5 };
            assert!(!m.observe(x), "baseline must not flag");
        }
        // A large spike is anomalous.
        assert!(m.observe(1000.0), "spike must be flagged");
        assert_eq!(m.anomalies(), 1);
    }

    #[test]
    fn drift_flags_deviation_from_constant_baseline() {
        // A perfectly stable baseline (zero variance) then a jump → anomaly.
        let mut m = DriftMonitor::new(3.0);
        for _ in 0..12 {
            assert!(!m.observe(2.0), "constant baseline must not flag");
        }
        assert!(m.observe(5000.0), "a jump from a stable baseline is drift");
        assert_eq!(m.anomalies(), 1);
    }

    #[test]
    fn drift_no_anomaly_during_warmup() {
        let mut m = DriftMonitor::new(3.0);
        // Even a wild value within the warm-up window is not flagged.
        assert!(!m.observe(1.0));
        assert!(!m.observe(1000.0));
    }
}
