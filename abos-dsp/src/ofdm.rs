use num_complex::Complex64;
use rustfft::FftPlanner;
use rustfft::num_traits::Zero;

pub struct OFDMModulator {
    pub n_subcarriers: usize,
    pub cp_length: usize,
    pub pilot_carriers: Vec<usize>,
}

impl OFDMModulator {
    pub fn new(n_subcarriers: usize, cp_length: usize, pilot_carriers: Vec<usize>) -> Self {
        Self { n_subcarriers, cp_length, pilot_carriers }
    }

    pub fn modulate(&mut self, symbols: &[Complex64]) -> Vec<Complex64> {
        let mut freq_domain = vec![Complex64::zero(); self.n_subcarriers];
        let mut sym_idx = 0;
        for i in 0..self.n_subcarriers {
            if self.pilot_carriers.contains(&i) {
                freq_domain[i] = Complex64::new(1.0, 0.0);
            } else if sym_idx < symbols.len() {
                freq_domain[i] = symbols[sym_idx];
                sym_idx += 1;
            }
        }
        let mut planner = FftPlanner::<f64>::new();
        let fft = planner.plan_fft_inverse(self.n_subcarriers);
        fft.process(&mut freq_domain);
        let inv_n = 1.0 / self.n_subcarriers as f64;
        for v in freq_domain.iter_mut() {
            *v = *v * inv_n;
        }
        let mut result = Vec::with_capacity(self.n_subcarriers + self.cp_length);
        let cp_start = self.n_subcarriers - self.cp_length;
        for i in cp_start..self.n_subcarriers {
            result.push(freq_domain[i]);
        }
        result.extend_from_slice(&freq_domain);
        result
    }
}

pub struct OFDMDemodulator {
    pub n_subcarriers: usize,
    pub cp_length: usize,
    pub pilot_carriers: Vec<usize>,
}

impl OFDMDemodulator {
    pub fn new(n_subcarriers: usize, cp_length: usize, pilot_carriers: Vec<usize>) -> Self {
        Self { n_subcarriers, cp_length, pilot_carriers }
    }

    pub fn demodulate(&mut self, samples: &[Complex64]) -> (Vec<Complex64>, Vec<Complex64>) {
        let mut time_domain: Vec<Complex64> = samples[self.cp_length..self.n_subcarriers + self.cp_length].to_vec();
        let mut planner = FftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(self.n_subcarriers);
        fft.process(&mut time_domain);
        let mut data = Vec::new();
        let mut pilots = Vec::new();
        for i in 0..self.n_subcarriers {
            if self.pilot_carriers.contains(&i) {
                pilots.push(time_domain[i]);
            } else {
                data.push(time_domain[i]);
            }
        }
        (data, pilots)
    }
}