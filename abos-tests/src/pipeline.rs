//! Digital TX/RX pipeline composed from the workspace's real components.
//!
//! Re-exports the production modem from `abos_phy::modem`.

pub use abos_phy::modem::*;

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
            assert_eq!(recovered, payload, "payload must roundtrip byte-identical");
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

    #[test]
    fn length_prefix_rejects_truncation() {
        let mut modem = Modem::new();
        let payload = vec![0xEE; 64];
        let mut samples = modem.transmit(&payload);
        samples.truncate(samples.len() / 2);
        assert!(modem.receive(&samples, 0.0).is_err());
    }

    #[test]
    fn noiseless_llr_signs_match_convention() {
        use abos_fec::soft_decision::qpsk_llr;
        use num_complex::Complex64;
        // bit=1 maps to +1 axis; qpsk_llr must return positive LLR there.
        let (llr0, llr1) = qpsk_llr(Complex64::new(1.0, -1.0), 0.1);
        assert!(llr0 > 0.0, "I-axis +1 -> bit0 = 1");
        assert!(llr1 < 0.0, "Q-axis -1 -> bit1 = 0");
    }
}
