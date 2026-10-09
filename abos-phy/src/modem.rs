//! Digital modem implementing full PHY layer modulation and demodulation.
//!
//! Chain:
//! - **TX**: framing (u32 length) -> scramble -> per-32-byte LDPC block encode ->
//!   bit interleave -> QPSK mapping -> OFDM multi-carrier modulation -> optional DSSS spreading.
//! - **RX**: optional DSSS despreading -> OFDM demodulation -> soft QPSK LLR computation ->
//!   deinterleaving -> LDPC sum-product belief propagation decoding -> descrambling -> unframing.

use crate::dsss::{DSSSDemodulator, DSSSModulator};
use crate::scrambler::Scrambler;
use abos_common::error::{Error, Result};
use abos_dsp::ofdm::{OFDMDemodulator, OFDMModulator};
use abos_fec::interleaver::Interleaver;
use abos_fec::ldpc::LDPCCode;
use abos_fec::soft_decision::qpsk_llr;
use num_complex::Complex64;

/// Max payload bytes per LDPC block (k = 256 bits).
pub const BLOCK_BYTES: usize = 32;
/// LDPC codeword length in bits (n = 512).
pub const CODEWORD_BITS: usize = 512;
/// QPSK symbols per codeword.
pub const SYMBOLS_PER_CODEWORD: usize = CODEWORD_BITS / 2;
/// OFDM subcarrier count.
pub const N_SUBCARRIERS: usize = 256;
/// OFDM cyclic prefix length.
pub const CP_LENGTH: usize = 32;
/// OFDM pilot carrier indices.
pub const PILOTS: [usize; 4] = [0, 64, 128, 192];
/// Available data carriers per OFDM symbol.
pub const DATA_CARRIERS: usize = N_SUBCARRIERS - PILOTS.len(); // 252

/// Full digital modem for baseband I/Q sample generation and recovery.
pub struct Modem {
    ldpc: LDPCCode,
    interleaver: Interleaver,
    ofdm_mod: OFDMModulator,
    ofdm_demod: OFDMDemodulator,
    dsss_chips: Option<usize>,
    dsss_seed: [u8; 32],
}

impl Modem {
    /// Create a standard modem without DSSS spreading.
    pub fn new() -> Self {
        Self::with_dsss(None)
    }

    /// Create a modem with optional DSSS spreading and a default seed.
    pub fn with_dsss(chips_per_symbol: Option<usize>) -> Self {
        Self::with_seed(chips_per_symbol, [7u8; 32])
    }

    /// Create a modem with optional DSSS spreading and custom seed.
    pub fn with_seed(chips_per_symbol: Option<usize>, seed: [u8; 32]) -> Self {
        Self {
            ldpc: LDPCCode::new(256, 512),
            interleaver: Interleaver::new(CODEWORD_BITS, 42),
            ofdm_mod: OFDMModulator::new(N_SUBCARRIERS, CP_LENGTH, PILOTS.to_vec()),
            ofdm_demod: OFDMDemodulator::new(N_SUBCARRIERS, CP_LENGTH, PILOTS.to_vec()),
            dsss_chips: chips_per_symbol,
            dsss_seed: seed,
        }
    }

    /// Set DSSS spreading parameters.
    pub fn set_dsss(&mut self, chips_per_symbol: Option<usize>, seed: [u8; 32]) {
        self.dsss_chips = chips_per_symbol;
        self.dsss_seed = seed;
    }

    /// Modulate raw byte payload into baseband I/Q samples.
    pub fn transmit(&mut self, payload: &[u8]) -> Vec<Complex64> {
        let mut frame = Vec::with_capacity(payload.len() + 4);
        frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        frame.extend_from_slice(payload);
        let mut scrambler = Scrambler::default();
        let scrambled = scrambler.scramble(&frame);

        let mut samples = Vec::with_capacity(
            scrambled.len().div_ceil(BLOCK_BYTES)
                * ((SYMBOLS_PER_CODEWORD.div_ceil(DATA_CARRIERS)) * (N_SUBCARRIERS + CP_LENGTH)),
        );

        for block in scrambled.chunks(BLOCK_BYTES) {
            let codeword = self.ldpc.encode(block);
            let bits = unpack_bits(&codeword, CODEWORD_BITS);
            let interleaved = self.interleaver.interleave(&bits);

            let symbols: Vec<Complex64> = interleaved
                .chunks(2)
                .map(|pair| {
                    let re = if pair[0] == 1 { 1.0 } else { -1.0 };
                    let im = if pair[1] == 1 { 1.0 } else { -1.0 };
                    Complex64::new(re, im)
                })
                .collect();

            for group in symbols.chunks(DATA_CARRIERS) {
                samples.extend(self.ofdm_mod.modulate(group));
            }
        }

        if let Some(chips) = self.dsss_chips {
            let mut spreader = DSSSModulator::new(&self.dsss_seed, chips);
            samples = spreader.spread(&samples);
        }

        samples
    }

    /// Demodulate baseband I/Q samples into decoded byte payload.
    pub fn receive(&mut self, samples: &[Complex64], noise_variance: f64) -> Result<Vec<u8>> {
        let samples = if let Some(chips) = self.dsss_chips {
            let mut despreader = DSSSDemodulator::new(&self.dsss_seed, chips);
            despreader.despread(samples)
        } else {
            samples.to_vec()
        };

        let block_samples = N_SUBCARRIERS + CP_LENGTH;
        if samples.is_empty() || samples.len() % block_samples != 0 {
            return Err(Error::ProtocolError(format!(
                "Sample stream {} is not a whole number of OFDM blocks",
                samples.len()
            )));
        }

        let mut decoded_blocks: Vec<Vec<u8>> = Vec::new();

        for pair in samples.chunks(2 * block_samples) {
            let mut llrs: Vec<f64> = Vec::with_capacity(CODEWORD_BITS);

            for block in pair.chunks(block_samples) {
                let (data_syms, _pilots) = self.ofdm_demod.demodulate(block);
                for sym in &data_syms {
                    let (llr0, llr1) = qpsk_llr(*sym, noise_variance);
                    llrs.push(llr0);
                    llrs.push(llr1);
                    if llrs.len() == CODEWORD_BITS {
                        break;
                    }
                }
                if llrs.len() == CODEWORD_BITS {
                    break;
                }
            }

            if llrs.len() < CODEWORD_BITS {
                return Err(Error::ProtocolError(
                    "Incomplete codeword in sample stream".into(),
                ));
            }

            let deinterleaved = self.deinterleave_llrs(&llrs);
            let block_bytes = self.ldpc.decode(&deinterleaved, 50)?;
            decoded_blocks.push(block_bytes);
        }

        let scrambled: Vec<u8> = decoded_blocks.into_iter().flatten().collect();
        let mut scrambler = Scrambler::default();
        let stream = scrambler.descramble(&scrambled);
        if stream.len() < 4 {
            return Err(Error::ProtocolError("Frame too short".into()));
        }
        let payload_len = u32::from_le_bytes([stream[0], stream[1], stream[2], stream[3]]) as usize;
        if stream.len() < 4 + payload_len {
            return Err(Error::ProtocolError(format!(
                "Truncated frame: header says {} bytes, have {}",
                payload_len,
                stream.len() - 4
            )));
        }
        Ok(stream[4..4 + payload_len].to_vec())
    }

    fn deinterleave_llrs(&self, llrs: &[f64]) -> Vec<f64> {
        let mut out = vec![0.0; llrs.len()];
        for (i, &pos) in self.interleaver.permutation.iter().enumerate() {
            if i < llrs.len() && pos < llrs.len() {
                out[i] = llrs[pos];
            }
        }
        out
    }
}

impl Default for Modem {
    fn default() -> Self {
        Self::new()
    }
}

fn unpack_bits(bytes: &[u8], count: usize) -> Vec<u8> {
    let mut bits = Vec::with_capacity(count);
    for i in 0..count {
        let b = (bytes[i / 8] >> (i % 8)) & 0x01;
        bits.push(b);
    }
    bits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_roundtrip_byte_identical() {
        let mut modem = Modem::new();
        for payload in [
            vec![],
            vec![0u8],
            b"hello ionosphere".to_vec(),
            (0..=255u8).collect::<Vec<u8>>(),
            vec![0xAB; 32],
            vec![0xCD; 100],
        ] {
            let samples = modem.transmit(&payload);
            let recovered = modem
                .receive(&samples, 0.0)
                .expect("clean channel must decode");
            assert_eq!(recovered, payload);
        }
    }

    #[test]
    fn roundtrip_with_dsss_spreading() {
        let mut modem = Modem::with_dsss(Some(4));
        let payload = b"spread spectrum payload".to_vec();
        let samples = modem.transmit(&payload);
        assert!(samples.len() > 2 * (N_SUBCARRIERS + CP_LENGTH));
        let recovered = modem.receive(&samples, 0.0).expect("decode");
        assert_eq!(recovered, payload);
    }
}
