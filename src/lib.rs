//! # Atmospheric Broadcast OS (ABOS)
//!
//! A software-defined radio operating system written in Rust that enables
//! covert, resilient, long-range communication by exploiting ionospheric
//! propagation physics. The atmosphere itself becomes a distributed relay,
//! memory buffer, and stealth medium.
//!
//! ## Architecture
//!
//! See `WHITEPAPER.md` or the documentation portal (`docs.html`) for
//! the complete system specification and RF chain design.

use abos_hal::dma::DMABuffer;
use abos_hal::gpio::GPIO;
use abos_hal::sdr::{create_sdr, SDRDevice, SDRType};
use abos_hal::time::GPSDOTimer;

use abos_dsp::agc::AGC;
use abos_dsp::costas::CostasLoop;
use abos_dsp::ddc::DDC;
use abos_dsp::iq_correct::IQCorrect;
use abos_dsp::ofdm::{OFDMDemodulator, OFDMModulator};
use abos_dsp::pulse_shape::RRCFilter;
use abos_dsp::timing::GardnerTiming;

use abos_phy::dsss::{DSSSDemodulator, DSSSModulator};
use abos_phy::fhss::FHSSEngine;
use abos_phy::modem::Modem;

use abos_fec::interleaver::Interleaver;
use abos_fec::ldpc::LDPCCode;

use abos_protocol::bounce::BufferBounceEngine;
use abos_protocol::bundle::{
    create_bundle, decrypt_bundle, deserialize_bundle, encrypt_bundle, serialize_bundle,
};
use abos_protocol::mesh::{Beacon, MeshNode};
use abos_protocol::routing::DedupCache;
use abos_protocol::schedule::PhoenixScheduler;
use abos_protocol::shard::split_file;

use abos_cognitive::adapt::AdaptiveController;
use abos_cognitive::jammer_detect::detect_jammer;
use abos_cognitive::scanner::SpectrumScanner;
use abos_cognitive::whitespace::find_white_space;

use abos_iono::fo_f2::estimate_fo_f2;
use abos_iono::meteor::detect_meteor_burst;
use abos_iono::nvis::select_nvis_frequency;
use abos_iono::propagation::predict_muf;
use abos_iono::sounder::ChirpSounder;

use abos_stealth::amp_dither::AmpDither;
use abos_stealth::burst_rand::RandomBurstScheduler;
use abos_stealth::phase_noise::PhaseNoiseInjector;

use abos_storage::bundle_store::BundleStore;
use abos_storage::config::SystemConfig;

use abos_common::crypto::{derive_key, Keypair};
use abos_common::error::{Error, Result};
use abos_common::types::*;

use num_complex::Complex64;

/// Top-level ABOS system orchestrator.
///
/// Wires together all layers: SDR hardware → DSP chain → PHY → FEC →
/// Protocol → Storage → Cognitive → Stealth → Ionospheric sounding.
pub struct ABOSSystem {
    config: SystemConfig,
    #[allow(dead_code)]
    keypair: Keypair,
    sdr: Box<dyn SDRDevice>,
    modem: Modem,
    #[allow(dead_code)]
    dma_buffer: DMABuffer,
    gpio: GPIO,
    #[allow(dead_code)]
    timer: GPSDOTimer,
    #[allow(dead_code)]
    ddc: DDC,
    #[allow(dead_code)]
    agc: AGC,
    #[allow(dead_code)]
    iq_correct: IQCorrect,
    #[allow(dead_code)]
    costas: CostasLoop,
    #[allow(dead_code)]
    timing: GardnerTiming,
    #[allow(dead_code)]
    ofdm_mod: OFDMModulator,
    #[allow(dead_code)]
    ofdm_demod: OFDMDemodulator,
    #[allow(dead_code)]
    rrc: RRCFilter,
    #[allow(dead_code)]
    dsss_mod: Option<DSSSModulator>,
    #[allow(dead_code)]
    dsss_demod: Option<DSSSDemodulator>,
    #[allow(dead_code)]
    fhss: Option<FHSSEngine>,
    #[allow(dead_code)]
    ldpc: LDPCCode,
    #[allow(dead_code)]
    interleaver: Interleaver,
    scanner: SpectrumScanner,
    adaptive: AdaptiveController,
    bounce: BufferBounceEngine,
    #[allow(dead_code)]
    scheduler: PhoenixScheduler,
    dedup: DedupCache,
    mesh: MeshNode,
    bundle_store: Option<BundleStore>,
    phase_noise: PhaseNoiseInjector,
    amp_dither: AmpDither,
    burst_scheduler: RandomBurstScheduler,
    running: bool,
    node_id: [u8; 32],
    shared_seed: [u8; 32],
}

impl ABOSSystem {
    /// Create a new ABOS system with default configuration.
    pub async fn new() -> Result<Self> {
        let config = SystemConfig::load()?;
        Self::with_config(config).await
    }

    /// Create a new ABOS system with a specific configuration and default SDR.
    pub async fn with_config(config: SystemConfig) -> Result<Self> {
        let sdr = create_sdr(SDRType::LimeSDR);
        Self::with_sdr(config, sdr).await
    }

    /// Create a new ABOS system with a custom SDR backend (e.g. LoopbackSDR for tests or simulation).
    pub async fn with_sdr(mut config: SystemConfig, mut sdr: Box<dyn SDRDevice>) -> Result<Self> {
        let keypair = config.load_or_create_identity()?;
        let node_id = keypair.node_id();
        let shared_seed = config.shared_seed;

        // Configure SDR
        let sdr_config = SDRConfig {
            center_frequency: config.center_frequency,
            sample_rate: config.sample_rate,
            gain: config.rx_gain,
            bandwidth: config.bandwidth,
            antenna_port: 0,
        };
        sdr.configure(sdr_config)?;

        let modem = Modem::with_seed(config.dsss_chips, shared_seed);

        let agc = AGC::default();
        let iq_correct = IQCorrect::default();
        let ddc = DDC::new(config.center_frequency as f64, config.sample_rate);
        let costas = CostasLoop::new(0.01);
        let timing = GardnerTiming::new(4);
        let rrc = RRCFilter::new(4, 0.35, 32);
        let ofdm_mod = OFDMModulator::new(256, 32, vec![0, 64, 128, 192]);
        let ofdm_demod = OFDMDemodulator::new(256, 32, vec![0, 64, 128, 192]);
        let ldpc = LDPCCode::new(256, 512);
        let interleaver = Interleaver::new(512, 42);
        let scanner = SpectrumScanner::new(1024);
        let adaptive = AdaptiveController::new(config.default_mcs, 1000.0);
        let bounce = BufferBounceEngine::new(5);
        let scheduler = PhoenixScheduler::new(10.0);
        let dedup = DedupCache::new(1000);
        let mesh = MeshNode::new(node_id);
        let phase_noise = PhaseNoiseInjector::new(0.01);
        let amp_dither = AmpDither::new(0.05);
        let burst_scheduler = RandomBurstScheduler::new(100, 5000);

        let mut dma_buffer = DMABuffer::new(65536);
        dma_buffer.set_watermark(32768);

        let gpio = GPIO::new();
        let timer = GPSDOTimer::new(10_000_000.0);

        let store_path = config.data_dir.join("bundles.sled");

        let mut system = Self {
            config,
            keypair,
            sdr,
            modem,
            dma_buffer,
            gpio,
            timer,
            ddc,
            agc,
            iq_correct,
            costas,
            timing,
            ofdm_mod,
            ofdm_demod,
            rrc,
            dsss_mod: None,
            dsss_demod: None,
            fhss: None,
            ldpc,
            interleaver,
            scanner,
            adaptive,
            bounce,
            scheduler,
            dedup,
            mesh,
            bundle_store: None,
            phase_noise,
            amp_dither,
            burst_scheduler,
            running: false,
            node_id,
            shared_seed,
        };

        if let Some(p) = store_path.to_str() {
            let _ = system.init_bundle_store(p);
        }
        Ok(system)
    }

    /// Start the SDR stream and all processing chains.
    pub async fn start(&mut self) -> Result<()> {
        if self.running {
            return Ok(());
        }
        self.sdr.start_stream()?;
        self.gpio.set_pa_enable(true);
        self.gpio.set_tr_switch(false); // RX mode
        self.running = true;
        Ok(())
    }

    /// Stop the SDR stream and all processing chains.
    pub async fn stop(&mut self) -> Result<()> {
        if !self.running {
            return Ok(());
        }
        self.sdr.stop_stream()?;
        self.gpio.set_pa_enable(false);
        self.running = false;
        Ok(())
    }

    /// Transmit a payload through the full RF chain.
    ///
    /// Steps: Shard split → Encrypted DTN Bundle → Length frame → Scramble →
    /// LDPC encode → Bit interleave → QPSK → OFDM modulate → (optional DSSS spread) →
    /// (optional Stealth mask) → SDR TX
    pub async fn transmit(&mut self, data: &[u8]) -> Result<()> {
        if !self.running {
            return Err(Error::ConfigError("System not started".into()));
        }

        let shards = split_file(data, 1024, 1.5);

        for shard in &shards {
            let mut bundle = create_bundle(
                &bincode::serialize(shard).map_err(|e| Error::ProtocolError(e.to_string()))?,
                self.node_id,
                3600,
            );

            // Encrypt bundle payload with shared network secret derived from shared_seed
            let enc_key = derive_key(&self.shared_seed, b"abos-bundle-encryption");
            encrypt_bundle(&enc_key, &mut bundle)?;

            let serialized_bundle = serialize_bundle(&bundle)?;
            let mut samples = self.modem.transmit(&serialized_bundle);

            // Anti-EW stealth dither if enabled
            if self.config.stealth_enabled {
                samples = self.phase_noise.inject_noise(&samples);
                samples = self.amp_dither.apply_dither(&samples);
            }

            self.gpio.set_tr_switch(true); // TX mode
            self.sdr.write_samples(&samples)?;
            self.gpio.set_tr_switch(false); // Back to RX

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            self.bounce.submit_shard(shard.clone(), now);
            self.mesh.track_bundle(&bundle);
        }

        Ok(())
    }

    /// Receive and process IQ samples through the full RX chain.
    ///
    /// Steps: SDR RX → (optional DSSS despread) → OFDM demod → Soft QPSK LLR →
    /// Deinterleave → LDPC decode → Descramble → Length unframe → Bundle deserialize →
    /// AES Decrypt → Dedup check → Bundle store → Shard reconstruct
    pub async fn receive(&mut self, buffer: &mut [Complex64]) -> Result<Vec<u8>> {
        if !self.running {
            return Err(Error::ConfigError("System not started".into()));
        }

        let n = self.sdr.read_samples(buffer)?;
        if n == 0 {
            return Err(Error::Timeout);
        }

        let samples = &buffer[..n];
        let decoded_bytes = self.modem.receive(samples, 0.0)?;
        let mut bundle = deserialize_bundle(&decoded_bytes)?;

        // Decrypt bundle payload
        let enc_key = derive_key(&self.shared_seed, b"abos-bundle-encryption");
        decrypt_bundle(&enc_key, &mut bundle)?;

        if self.dedup.check_and_insert(bundle.bundle_id) {
            return Err(Error::ProtocolError("Duplicate bundle".into()));
        }

        let is_new = if let Some(ref store) = self.bundle_store {
            store.store_bundle(&bundle)?
        } else {
            true
        };

        let s: Shard = bincode::deserialize(&bundle.payload)
            .map_err(|e| Error::ProtocolError(e.to_string()))?;

        self.bounce.record_complementary_shard(s.clone());
        self.mesh
            .record_availability_from_shard(&s, bundle.source_node);

        // Re-broadcast and track ACKs if flooding policy permits
        if is_new && self.mesh.should_forward_bundle(&bundle) {
            let mut fwd = bundle.clone();
            abos_protocol::routing::increment_hop(&mut fwd);
            let fshard: Shard = bincode::deserialize(&fwd.payload)
                .map_err(|e| Error::ProtocolError(e.to_string()))?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            self.bounce.submit_shard(fshard, now);
            self.mesh.track_bundle(&fwd);
        }

        Ok(bundle.payload)
    }

    /// Scan the spectrum and return the power spectral density.
    pub fn scan_spectrum(&mut self, samples: &[Complex64]) -> Vec<f64> {
        self.scanner.scan(samples)
    }

    /// Detect jammers in the current spectrum.
    pub fn detect_interferers(&self, spectrum: &[f64], threshold: f64) -> Vec<usize> {
        detect_jammer(spectrum, threshold)
    }

    /// Find whitespace regions in the spectrum.
    pub fn find_whitespace(&self, spectrum: &[f64], noise_floor: f64) -> Vec<(usize, usize)> {
        find_white_space(spectrum, noise_floor)
    }

    /// Update adaptive modulation based on SNR and BER.
    pub fn adapt_to_channel(&mut self, snr: f64, ber: f64) -> MCS {
        self.adaptive.adapt_to_channel(snr, ber)
    }

    /// Get the current node ID.
    pub fn node_id(&self) -> [u8; 32] {
        self.node_id
    }

    /// Get the system config.
    pub fn config(&self) -> &SystemConfig {
        &self.config
    }

    /// Get a mutable reference to the system config.
    pub fn config_mut(&mut self) -> &mut SystemConfig {
        &mut self.config
    }

    /// Check if the system is running.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Initialize DSSS spreading.
    pub fn init_dsss(&mut self, chips_per_symbol: usize) {
        self.modem
            .set_dsss(Some(chips_per_symbol), self.shared_seed);
        self.dsss_mod = Some(DSSSModulator::new(&self.shared_seed, chips_per_symbol));
        self.dsss_demod = Some(DSSSDemodulator::new(&self.shared_seed, chips_per_symbol));
    }

    /// Initialize FHSS.
    pub fn init_fhss(&mut self, hop_duration: f64, min_freq: f64, max_freq: f64, num_hops: usize) {
        self.fhss = Some(FHSSEngine::new(
            &self.shared_seed,
            hop_duration,
            min_freq,
            max_freq,
            num_hops,
        ));
    }

    /// Initialize the bundle store with a path.
    pub fn init_bundle_store(&mut self, path: &str) -> Result<()> {
        let store = BundleStore::new(path)?;
        self.bundle_store = Some(store);
        Ok(())
    }

    /// Estimate foF2 from electron density.
    pub fn estimate_fo_f2(electron_density: f64) -> f64 {
        estimate_fo_f2(electron_density)
    }

    /// Select optimal NVIS frequency based on foF2 and time of day.
    pub fn select_nvis_freq(fo_f2: f64, time_of_day: f64) -> f64 {
        select_nvis_frequency(fo_f2, time_of_day)
    }

    /// Predict MUF for a given distance.
    pub fn predict_muf(fo_f2: f64, distance_km: f64) -> f64 {
        predict_muf(fo_f2, distance_km)
    }

    /// Generate a chirp sounder waveform.
    pub fn generate_chirp(start_freq: f64, stop_freq: f64, duration: f64) -> Vec<Complex64> {
        let sounder = ChirpSounder::new(start_freq, stop_freq, duration);
        sounder.generate_chirp()
    }

    /// Detect a meteor burst from SNR history.
    pub fn detect_meteor(snr_history: &[f64]) -> bool {
        detect_meteor_burst(snr_history)
    }

    /// Get next scheduled burst time (stealth).
    pub fn next_burst_time(&mut self) -> u64 {
        self.burst_scheduler.next_burst_time()
    }

    // --- Mesh / Ghost-node coordination ---------------------------------

    /// Immutable view of the mesh state (peer list, pending ACKs, …).
    pub fn mesh(&self) -> &MeshNode {
        &self.mesh
    }

    /// Mutable mesh handle (drives beacons, ACKs, backoff sweeps).
    pub fn mesh_mut(&mut self) -> &mut MeshNode {
        &mut self.mesh
    }

    /// Emit a local discovery beacon if the interval has elapsed.
    /// Returns the beacon to flood, or `None` if it is not due yet.
    pub fn maybe_emit_beacon(&mut self, alias: &str) -> Option<Beacon> {
        if self.mesh.beacon_due() {
            Some(self.mesh.emit_beacon(alias))
        } else {
            None
        }
    }

    /// Handle an inbound beacon; returns it if it should be re-flooded.
    pub fn on_beacon(&mut self, beacon: &Beacon) -> Option<Beacon> {
        if self.mesh.on_beacon(beacon) {
            Some(beacon.clone())
        } else {
            None
        }
    }

    /// Handle an inbound ACK for a bundle we originated.
    pub fn on_ack(&mut self, bundle_id: [u8; 32], peer: NodeId) -> bool {
        self.mesh.record_ack(bundle_id, peer)
    }

    /// Bundles due for retransmission under exponential backoff.
    pub fn retransmit_queue(&mut self) -> Vec<[u8; 32]> {
        self.mesh.due_for_retransmit()
    }

    /// Expire stale peers; returns how many were dropped.
    pub fn expire_mesh_peers(&mut self) -> usize {
        self.mesh.expire_peers()
    }

    /// Evict expired bundles from the persistent store.
    pub fn evict_expired_bundles(&mut self) -> usize {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        match &self.bundle_store {
            Some(store) => store.evict_expired(now).unwrap_or(0),
            None => 0,
        }
    }

    /// Number of bundles in the persistent store.
    pub fn stored_bundle_count(&self) -> usize {
        self.bundle_store.as_ref().map(|s| s.len()).unwrap_or(0)
    }
}

impl Drop for ABOSSystem {
    fn drop(&mut self) {
        let _ = self.sdr.stop_stream();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use abos_hal::sdr::LoopbackSDR;

    #[tokio::test]
    async fn test_system_creation() {
        let system = ABOSSystem::new().await;
        assert!(system.is_ok());
    }

    #[test]
    fn test_estimate_fo_f2() {
        let fo_f2 = ABOSSystem::estimate_fo_f2(1e12);
        assert!((fo_f2 - 9_000_000.0).abs() < 1.0);
    }

    #[test]
    fn test_select_nvis_frequency() {
        let freq = ABOSSystem::select_nvis_freq(7.0e6, 12.0);
        assert!(freq > 0.0);
        assert!(freq < 7.0e6);
    }

    #[test]
    fn test_generate_chirp() {
        let chirp = ABOSSystem::generate_chirp(3.0e6, 10.0e6, 0.1);
        assert!(!chirp.is_empty());
    }

    #[test]
    fn test_detect_meteor() {
        let mut history = vec![5.0; 10];
        history.push(25.0);
        assert!(ABOSSystem::detect_meteor(&history));
        assert!(!ABOSSystem::detect_meteor(&[1.0, 2.0]));
    }

    #[tokio::test]
    async fn test_two_nodes_loopback_roundtrip() {
        let (sdr_a, sdr_b) = LoopbackSDR::pair();

        let config_a = SystemConfig {
            data_dir: std::env::temp_dir().join(format!("abos_test_a_{}", std::process::id())),
            ..Default::default()
        };
        let config_b = SystemConfig {
            data_dir: std::env::temp_dir().join(format!("abos_test_b_{}", std::process::id())),
            ..Default::default()
        };

        let mut node_a = ABOSSystem::with_sdr(config_a, Box::new(sdr_a))
            .await
            .unwrap();
        let mut node_b = ABOSSystem::with_sdr(config_b, Box::new(sdr_b))
            .await
            .unwrap();

        node_a.start().await.unwrap();
        node_b.start().await.unwrap();

        let payload = b"Hello from Node A across atmospheric ionosphere!";
        node_a.transmit(payload).await.unwrap();

        let mut rx_buf = vec![Complex64::default(); 65536];
        let received = node_b
            .receive(&mut rx_buf)
            .await
            .expect("Node B should receive message");

        let shard: Shard = bincode::deserialize(&received).expect("Deserialization of shard");
        assert_eq!(&shard.data[..payload.len()], payload);
    }
}
