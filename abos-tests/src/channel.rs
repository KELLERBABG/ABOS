//! Deterministic in-memory channel with configurable impairments.

use abos_common::types::Bundle;
use num_complex::Complex64;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

/// A reproducible link between simulated nodes.
///
/// Impairments are applied independently per item:
/// - **loss** — the item is dropped entirely,
/// - **duplication** — the item is delivered twice (exercises dedup),
/// - **corruption** — one payload byte is flipped (exercises checksums).
///
/// All randomness comes from a seeded RNG, so every test run is identical.
pub struct LoopbackChannel {
    rng: StdRng,
    /// Probability \[0,1\] that an item is dropped.
    pub loss_rate: f64,
    /// Probability \[0,1\] that an item is delivered twice.
    pub duplicate_rate: f64,
    /// Probability \[0,1\] that an item's payload is corrupted.
    pub corruption_rate: f64,
    /// RMS noise added to I/Q samples (0.0 = clean).
    pub noise_rms: f64,
}

impl LoopbackChannel {
    /// A perfect channel: no loss, no corruption, no noise.
    pub fn clean() -> Self {
        Self {
            rng: StdRng::seed_from_u64(0),
            loss_rate: 0.0,
            duplicate_rate: 0.0,
            corruption_rate: 0.0,
            noise_rms: 0.0,
        }
    }

    /// A channel with the given impairments, seeded for reproducibility.
    pub fn seeded(
        seed: u64,
        loss_rate: f64,
        duplicate_rate: f64,
        corruption_rate: f64,
        noise_rms: f64,
    ) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            loss_rate,
            duplicate_rate,
            corruption_rate,
            noise_rms,
        }
    }

    fn roll(&mut self, p: f64) -> bool {
        p > 0.0 && self.rng.gen::<f64>() < p
    }

    /// Deliver one bundle through the channel, returning every copy that
    /// arrives (0 = lost, 1 = normal, 2 = duplicated).
    pub fn deliver_bundle(&mut self, bundle: &Bundle) -> Vec<Bundle> {
        if self.roll(self.loss_rate) {
            return Vec::new();
        }
        let mut arrived = bundle.clone();
        if self.roll(self.corruption_rate) {
            if let Some(byte) = arrived.payload.last_mut() {
                *byte ^= 0xFF; // flip all bits of the last payload byte
            }
        }
        let mut out = vec![arrived.clone()];
        if self.roll(self.duplicate_rate) {
            out.push(arrived);
        }
        out
    }

    /// Push I/Q samples through the channel, adding AWGN when configured.
    pub fn deliver_samples(&mut self, samples: &[Complex64]) -> Vec<Complex64> {
        if self.noise_rms <= 0.0 {
            return samples.to_vec();
        }
        samples
            .iter()
            .map(|s| {
                let n_re = self.gaussian() * self.noise_rms;
                let n_im = self.gaussian() * self.noise_rms;
                Complex64::new(s.re + n_re, s.im + n_im)
            })
            .collect()
    }

    /// Standard normal sample via Box–Muller.
    fn gaussian(&mut self) -> f64 {
        let u1: f64 = self.rng.gen::<f64>().max(f64::EPSILON);
        let u2: f64 = self.rng.gen();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

/// An ionospheric multipath path in the Watterson channel model (ITU-R 520).
#[derive(Debug, Clone)]
pub struct WattersonPath {
    /// Relative delay of this path in samples.
    pub delay_samples: usize,
    /// Linear amplitude gain of this path (e.g. 1.0).
    pub gain: f64,
    /// Doppler shift in Hz.
    pub doppler_shift_hz: f64,
    /// Doppler 2-sigma spread in Hz (Rayleigh fading speed).
    pub doppler_spread_hz: f64,
}

/// ITU-R 520 Watterson HF ionospheric multipath fading channel simulator.
///
/// Simulates NVIS and long-range HF skywave links with discrete ionospheric reflections,
/// differential multipath delay, Rayleigh fading, Doppler shift, and thermal AWGN.
pub struct WattersonChannel {
    paths: Vec<WattersonPath>,
    sample_rate: f64,
    noise_rms: f64,
    rng: StdRng,
}

impl WattersonChannel {
    /// Create a custom Watterson channel.
    pub fn new(paths: Vec<WattersonPath>, sample_rate: f64, noise_rms: f64, seed: u64) -> Self {
        Self {
            paths,
            sample_rate,
            noise_rms,
            rng: StdRng::seed_from_u64(seed),
        }
    }

    /// CCIR "Good" conditions: 2 paths, 0.5 ms differential delay, 0.1 Hz Doppler spread.
    pub fn ccir_good(sample_rate: f64, noise_rms: f64, seed: u64) -> Self {
        let delay_samples = (0.0005 * sample_rate) as usize;
        let paths = vec![
            WattersonPath {
                delay_samples: 0,
                gain: 1.0,
                doppler_shift_hz: 0.0,
                doppler_spread_hz: 0.1,
            },
            WattersonPath {
                delay_samples,
                gain: 0.7,
                doppler_shift_hz: 0.1,
                doppler_spread_hz: 0.1,
            },
        ];
        Self::new(paths, sample_rate, noise_rms, seed)
    }

    /// CCIR "Moderate" conditions: 2 paths, 1.0 ms differential delay, 0.5 Hz Doppler spread.
    pub fn ccir_moderate(sample_rate: f64, noise_rms: f64, seed: u64) -> Self {
        let delay_samples = (0.001 * sample_rate) as usize;
        let paths = vec![
            WattersonPath {
                delay_samples: 0,
                gain: 1.0,
                doppler_shift_hz: 0.0,
                doppler_spread_hz: 0.5,
            },
            WattersonPath {
                delay_samples,
                gain: 0.5,
                doppler_shift_hz: 0.5,
                doppler_spread_hz: 0.5,
            },
        ];
        Self::new(paths, sample_rate, noise_rms, seed)
    }

    /// CCIR "Poor" conditions: 2 paths, 2.0 ms differential delay, 1.0 Hz Doppler spread.
    pub fn ccir_poor(sample_rate: f64, noise_rms: f64, seed: u64) -> Self {
        let delay_samples = (0.002 * sample_rate) as usize;
        let paths = vec![
            WattersonPath {
                delay_samples: 0,
                gain: 1.0,
                doppler_shift_hz: 0.0,
                doppler_spread_hz: 1.0,
            },
            WattersonPath {
                delay_samples,
                gain: 0.5,
                doppler_shift_hz: 1.0,
                doppler_spread_hz: 1.0,
            },
        ];
        Self::new(paths, sample_rate, noise_rms, seed)
    }

    /// Deliver I/Q samples across the Watterson multipath channel.
    pub fn deliver_samples(&mut self, samples: &[Complex64]) -> Vec<Complex64> {
        if samples.is_empty() {
            return Vec::new();
        }

        let max_delay = self
            .paths
            .iter()
            .map(|p| p.delay_samples)
            .max()
            .unwrap_or(0);
        let out_len = samples.len() + max_delay;
        let mut out = vec![Complex64::new(0.0, 0.0); out_len];

        let paths = self.paths.clone();
        for path in &paths {
            let mut phase = self.rng.gen::<f64>() * 2.0 * std::f64::consts::PI;
            let phase_step = 2.0 * std::f64::consts::PI * path.doppler_shift_hz / self.sample_rate;

            for (i, &s) in samples.iter().enumerate() {
                phase += phase_step;
                // Rayleigh tap fading variation
                let fade_re = Self::sample_gaussian(&mut self.rng)
                    * (path.doppler_spread_hz * 0.01).max(0.05);
                let fade_im = Self::sample_gaussian(&mut self.rng)
                    * (path.doppler_spread_hz * 0.01).max(0.05);
                let tap = Complex64::new(
                    path.gain * phase.cos() + fade_re,
                    path.gain * phase.sin() + fade_im,
                );
                let prod = s * tap;
                let dest_idx = i + path.delay_samples;
                if dest_idx < out.len() {
                    out[dest_idx] += prod;
                }
            }
        }

        // Add AWGN thermal noise
        if self.noise_rms > 0.0 {
            for item in &mut out {
                let n_re = Self::sample_gaussian(&mut self.rng) * self.noise_rms;
                let n_im = Self::sample_gaussian(&mut self.rng) * self.noise_rms;
                *item += Complex64::new(n_re, n_im);
            }
        }

        out
    }

    fn sample_gaussian(rng: &mut StdRng) -> f64 {
        let u1: f64 = rng.gen::<f64>().max(f64::EPSILON);
        let u2: f64 = rng.gen();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle(id: u8) -> Bundle {
        Bundle {
            bundle_id: [id; 32],
            source_node: [1; 32],
            creation_timestamp: 0,
            lifetime_seconds: 3600,
            payload: vec![0x42; 8],
            hop_count: 0,
            ttl: 10,
        }
    }

    #[test]
    fn clean_channel_is_transparent() {
        let mut ch = LoopbackChannel::clean();
        let out = ch.deliver_bundle(&bundle(1));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].payload, bundle(1).payload);
    }

    #[test]
    fn loss_drops_item() {
        let mut ch = LoopbackChannel::seeded(7, 1.0, 0.0, 0.0, 0.0);
        assert!(ch.deliver_bundle(&bundle(1)).is_empty());
    }

    #[test]
    fn duplication_delivers_twice() {
        let mut ch = LoopbackChannel::seeded(7, 0.0, 1.0, 0.0, 0.0);
        assert_eq!(ch.deliver_bundle(&bundle(1)).len(), 2);
    }

    #[test]
    fn corruption_flips_a_byte() {
        let mut ch = LoopbackChannel::seeded(7, 0.0, 0.0, 1.0, 0.0);
        let out = ch.deliver_bundle(&bundle(1));
        assert_ne!(out[0].payload, bundle(1).payload, "payload must differ");
    }

    #[test]
    fn seeded_channel_is_deterministic() {
        let input: Vec<Complex64> = (0..64)
            .map(|i| Complex64::new(i as f64 * 0.1, -(i as f64) * 0.05))
            .collect();
        let a = LoopbackChannel::seeded(42, 0.5, 0.5, 0.5, 0.1).deliver_samples(&input);
        let b = LoopbackChannel::seeded(42, 0.5, 0.5, 0.5, 0.1).deliver_samples(&input);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.re.to_bits(), y.re.to_bits());
            assert_eq!(x.im.to_bits(), y.im.to_bits());
        }
    }

    #[test]
    fn clean_samples_untouched() {
        let input = vec![Complex64::new(1.0, -1.0); 16];
        let out = LoopbackChannel::clean().deliver_samples(&input);
        assert_eq!(out, input);
    }

    #[test]
    fn watterson_channel_reproducibility_and_propagation() {
        let input: Vec<Complex64> = (0..128)
            .map(|i| Complex64::new((i as f64 * 0.05).cos(), (i as f64 * 0.05).sin()))
            .collect();
        let mut ch1 = WattersonChannel::ccir_good(1_000_000.0, 0.01, 12345);
        let mut ch2 = WattersonChannel::ccir_good(1_000_000.0, 0.01, 12345);

        let out1 = ch1.deliver_samples(&input);
        let out2 = ch2.deliver_samples(&input);

        assert_eq!(out1.len(), out2.len());
        assert!(out1.len() >= input.len());
        for (a, b) in out1.iter().zip(out2.iter()) {
            assert_eq!(a.re.to_bits(), b.re.to_bits());
            assert_eq!(a.im.to_bits(), b.im.to_bits());
        }
    }
}
