use abos_common::pn_gen::PNGenerator;
use num_complex::Complex64;

/// DSSS Modulator: spreads symbols using a PN sequence
pub struct DSSSModulator {
    pn_gen: PNGenerator,
    pub chips_per_symbol: usize,
}

impl DSSSModulator {
    /// Create a new DSSS modulator with a crypto-deterministic seed
    pub fn new(seed: &[u8; 32], chips_per_symbol: usize) -> Self {
        Self {
            pn_gen: PNGenerator::new(seed),
            chips_per_symbol,
        }
    }

    /// Spread symbols by multiplying each symbol with `chips_per_symbol` chips
    pub fn spread(&mut self, symbols: &[Complex64]) -> Vec<Complex64> {
        let chips = self.pn_gen.generate_chips(symbols.len() * self.chips_per_symbol);
        let mut output = Vec::with_capacity(symbols.len() * self.chips_per_symbol);
        for (i, &sym) in symbols.iter().enumerate() {
            for j in 0..self.chips_per_symbol {
                let chip = chips[i * self.chips_per_symbol + j];
                output.push(sym * chip);
            }
        }
        output
    }
}

/// DSSS Demodulator: despreads using correlation with the PN sequence
pub struct DSSSDemodulator {
    pn_gen: PNGenerator,
    pub chips_per_symbol: usize,
    chip_buffer: Vec<Complex64>,
}

impl DSSSDemodulator {
    pub fn new(seed: &[u8; 32], chips_per_symbol: usize) -> Self {
        Self {
            pn_gen: PNGenerator::new(seed),
            chips_per_symbol,
            chip_buffer: Vec::with_capacity(chips_per_symbol),
        }
    }

    /// Despread samples by correlating with the PN sequence
    pub fn despread(&mut self, samples: &[Complex64]) -> Vec<Complex64> {
        let mut output = Vec::new();
        for &sample in samples {
            self.chip_buffer.push(sample);
            if self.chip_buffer.len() == self.chips_per_symbol {
                let chip = self.pn_gen.next_chip();
                let mut acc = Complex64::new(0.0, 0.0);
                for s in self.chip_buffer.drain(..) {
                    acc = acc + s * chip;
                }
                output.push(acc * (1.0 / self.chips_per_symbol as f64));
            }
        }
        output
    }
}

/// Early-Late gate code tracking for DSSS acquisition
pub struct CodeAcquisition {
    early_offset: usize,
    late_offset: usize,
    correlation_threshold: f64,
}

impl CodeAcquisition {
    pub fn new(early_offset: usize, late_offset: usize, threshold: f64) -> Self {
        Self {
            early_offset,
            late_offset,
            correlation_threshold: threshold,
        }
    }

    /// Correlate samples against the PN code at early, on-time, and late positions
    pub fn track(&self, samples: &[Complex64], pn_gen: &mut PNGenerator) -> f64 {
        let code_len = samples.len();
        let early_code = pn_gen.generate_chips(code_len + self.early_offset);
        let late_code = pn_gen.generate_chips(code_len + self.late_offset);

        let early_corr: Complex64 = samples.iter().zip(early_code.iter())
            .map(|(&s, &c)| s * c)
            .sum();
        let late_corr: Complex64 = samples.iter().zip(late_code.iter().skip(self.late_offset))
            .map(|(&s, &c)| s * c)
            .sum();

        early_corr.norm_sqr() - late_corr.norm_sqr()
    }

    /// Detect whether the code is acquired (correlation above threshold)
    pub fn is_acquired(&self, corr_power: f64) -> bool {
        corr_power > self.correlation_threshold
    }
}