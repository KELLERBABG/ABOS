# Atmospheric Broadcast OS (ABOS) — Detailed Building Plan

## Overview

ABOS is a software-defined radio (SDR) operating system written in Rust that enables covert, resilient, long-range communication by exploiting ionospheric propagation physics. It treats the atmosphere itself as a distributed relay, memory buffer, and stealth medium. The system combines **spread spectrum stealth**, **cognitive radio**, **delay-tolerant networking**, and **ionospheric sounding** into a unified platform for decentralized mesh communication without routing tables.

---

## 1. Architecture Layers & Build Order

The build must proceed **bottom-up**: physical layer first, then DSP, then protocol, then cognitive/stealth layers on top.

### Phase 1: Hardware Abstraction Layer (HAL) & SDR Interface
- Select SDR hardware platform (see §1.1)
- Implement Rust SDR driver / bindings
- DMA-based zero-copy I/Q streaming into userspace
- Ring buffer / memory-mapped I/O for samples
- GPIO for antenna switching, PA control, T/R switching

### Phase 2: Digital Down Conversion (DDC) & Signal Conditioning
- DDC: Mix I/Q stream with NCO (Numerically Controlled Oscillator) to baseband
- **SIMD-optimized FIR filter** (decimation) to reduce sample rate
- AGC (Automatic Gain Control) — prevent ADC saturation
- DC offset correction
- I/Q imbalance compensation (amplitude & phase mismatch correction)

### Phase 3: Synchronization (PLL / Costas Loop)
- **Costas Loop** for carrier frequency & phase recovery
- Symbol timing recovery (Gardner / Mueller-Müller)
- Frame synchronization (preamble correlator)
- Doppler shift tracking & compensation (frequency discriminator)

### Phase 4: OFDM Core
- FFT/IFFT-based subcarrier modulation
- Cyclic prefix insertion/removal
- Pilot-based channel estimation
- Adaptive modulation per subcarrier (QPSK → 64-QAM)

### Phase 5: Spread Spectrum Engine
- **DSSS**: PN sequence generator, spreading/despreading
- **FHSS**: Frequency hop scheduler, synchronized hopping pattern
- Seed management (krypto-deterministic)
- Code acquisition & tracking (early-late gate for DSSS)

### Phase 6: Error Correction (BICM + Soft-Decision)
- Forward Error Correction code (LDPC or Turbo or Polar)
- Bit interleaver (pseudo-random deterministic)
- Soft-decision LLR (Log-Likelihood Ratio) calculation
- Deinterleaver → FEC decoder

### Phase 7: Physical Frame / Burst Format
- Preamble + sync word design
- Header with MCS selection, shard ID, length
- Payload (encrypted, FEC-encoded, interleaved)
- Pulse shaping (Root-Raised Cosine) per symbol

### Phase 8: Protocol Transmission Engine
- File → Shard splitter (with redundancy, e.g., Fountain codes / RaptorQ)
- Shard encapsulation (Bundle Protocol / custom DTN format)
- DTN store-and-forward with persistent storage (SQLite / sled)
- Asynchronous Buffer-Bounce logic (listen for complementary shards + ACKs)
- "Phoenix Window" scheduler (meteor scatter opportunistic transmission)

### Phase 9: Cognitive Radio & Spectrum Management
- FFT-based wideband spectrum scanner
- Energy detection + cyclostationary feature detection (to find jammers)
- White space identification algorithm
- Dynamic carrier frequency selection
- Adaptive symbol rate & modulation

### Phase 10: Stealth / Anti-Detection
- DSSS under noise floor
- Cyclostationary signature masking:
  - Variable symbol rate (dithering)
  - Artificial phase noise injection
  - Pseudo-natural amplitude fluctuations (modeled on ionospheric scintillation)
  - Randomized burst timing
- Burst transmission (< 1 ms packets)

### Phase 11: Ionospheric Sounding Integration
- Interface with ionosonde data (or built-in chirp sounder)
- Real-time f₀F2 calculation from electron density
- MUF (Maximum Usable Frequency) prediction
- NVIS frequency selection (must be below f₀F2, typically 2-10 MHz)
- Meteor scatter detection (sudden SNR peaks on far channels)

### Phase 12: Beamforming (Antenna Array)
- Multi-channel phase-coherent SDR (requires synchronized ADCs)
- Phase shift calculation per antenna element
- Beam steering algorithm (point beam direction)
- Null steering (place null toward known interferers)

### Phase 13: Mesh / Ghost Node Coordination (future)
- Distributed node discovery (no central authority)
- Shard ACK aggregation
- Reputation / Sybil resistance (optional)
- Network-wide shard availability tracking

---

## 1.1 Hardware Platform Selection

| Component | Options | Recommendation |
|-----------|---------|---------------|
| **SDR** | Ettus USRP B210, LimeSDR, PlutoSDR, HackRF, custom FPGA+ADC | **USRP B210** (70 MHz - 6 GHz, 56 MHz BW, full-duplex, phase-coherent 2x2 MIMO) or **LimeSDR** (cheaper, open source) |
| **PA (Power Amp)** | HF amplifier 100W+ for NVIS | Required for reliable NVIS; needs T/R switching |
| **Antenna** | Horizontal dipole (λ/2 at ~3-7 MHz), low height (0.1-0.25 λ) | **Horizontal dipole at ~5-10m height** — critical for NVIS high-angle radiation |
| **Antenna Array** | 4-8 element phased array | Requires phase-matched cables, calibration |
| **Compute** | Raspberry Pi 4/5, NVIDIA Jetson, x86 embedded PC, or FPGA SoC | **x86 embedded** (e.g., LattePanda) or **FPGA SoC** (Zynq) for real-time DSP — Raspberry Pi may be underpowered for real-time OFDM + FEC |
| **GPS/GNSS** | u-blox module | For time synchronization, frequency disciplining, location |
| **Storage** | NVMe SSD or high-endurance SD | DTN bundles need persistent, fast storage |

---

## 2. Rust Crate / Module Structure (Proposed)

```
abos/
├── abos-hal/              # Hardware Abstraction Layer
│   ├── src/
│   │   ├── sdr.rs         # SDR driver abstraction (USRP, LimeSDR, etc.)
│   │   ├── dma.rs         # DMA buffer management
│   │   ├── gpio.rs        # Antenna switching, PA control
│   │   └── time.rs        # GPS-disciplined oscillator
├── abos-dsp/              # Digital Signal Processing
│   ├── src/
│   │   ├── ddc.rs         # Digital Down Conversion
│   │   ├── decimation.rs  # SIMD FIR + decimation
│   │   ├── agc.rs         # Automatic Gain Control
│   │   ├── iq_correct.rs  # I/Q imbalance correction
│   │   ├── costas.rs      # Costas Loop / PLL
│   │   ├── timing.rs      # Symbol timing recovery
│   │   ├── ofdm.rs        # OFDM modulator/demodulator
│   │   ├── fft.rs         # FFT/IFFT (wrapper around rustfft)
│   │   └── pulse_shape.rs # RRC filter
├── abos-phy/              # Physical Layer
│   ├── src/
│   │   ├── dsss.rs        # Direct Sequence Spread Spectrum
│   │   ├── fhss.rs        # Frequency Hopping
│   │   ├── scrambler.rs   # Data scrambling
│   │   ├── burst.rs       # Burst builder / parser
│   │   └── beamforming.rs # Antenna array phase control
├── abos-fec/              # Forward Error Correction
│   ├── src/
│   │   ├── ldpc.rs        # LDPC encoder/decoder (or turbo/polar)
│   │   ├── interleaver.rs # BICM interleaver
│   │   ├── soft_decision.rs # LLR computation
│   │   └── crc.rs         # CRC check
├── abos-protocol/         # Protocol Layer
│   ├── src/
│   │   ├── shard.rs       # File → Shard splitting
│   │   ├── bundle.rs      # DTN Bundle Protocol
│   │   ├── bounce.rs      # Buffer-Bounce logic
│   │   ├── schedule.rs    # Opportunistic scheduler
│   │   └── routing.rs     # Implicit routing / flooding
├── abos-cognitive/        # Cognitive Radio
│   ├── src/
│   │   ├── scanner.rs     # Spectrum scanner
│   │   ├── jammer_detect.rs # Jammer detection
│   │   ├── whitespace.rs  # White space finder
│   │   └── adapt.rs       # Adaptive rate/modulation controller
├── abos-iono/             # Ionospheric Sounding
│   ├── src/
│   │   ├── sounder.rs     # Chirp/Zadoff-Chu sounder
│   │   ├── foF2.rs        # f₀F2 estimation
│   │   ├── nvis.rs        # NVIS frequency selection
│   │   ├── meteor.rs      # Meteor scatter detection
│   │   └── propagation.rs # Propagation models (ITU-R)
├── abos-stealth/          # Anti-Detection / LPI
│   ├── src/
│   │   ├── mask_cyclo.rs  # Cyclostationary masking
│   │   ├── phase_noise.rs # Artificial phase noise
│   │   ├── amp_dither.rs  # Amplitude variation injection
│   │   └── burst_rand.rs  # Randomized burst timing
├── abos-storage/          # Persistent Storage
│   ├── src/
│   │   ├── bundle_store.rs # DTN bundle storage
│   │   └── config.rs       # System configuration
├── abos-common/           # Shared types & utilities
│   ├── src/
│   │   ├── complex.rs     # Complex number types (or use num-complex)
│   │   ├── pn_gen.rs      # PN sequence generators
│   │   ├── crypto.rs      # Encryption, key exchange
│   │   └── error.rs       # Error types
├── abos-cli/              # Command-line interface
├── abos-gui/              # Optional GUI (egui/iced based)
└── abos-tests/            # Integration tests & channel simulation
```

---

## 3. Critical Technical Decisions (Unresolved in Canvas)

### 3.1 Frequency Band Selection
- **NVIS requires 2-10 MHz** (below f₀F2, typically 3-7 MHz at night, 5-10 MHz daytime)
- **Meteor scatter** works best at 30-100 MHz (VHF, e.g., 50 MHz / 6m band)
- **Question**: Which primary band? Dual-band SDR needed for meteor scatter.
- **Legal**: These bands require amateur radio licenses or are in military spectrum. Pirate operation is illegal in most jurisdictions. Consider:
  - HAM radio bands (requires license)
  - ISM bands (13.56 MHz, 27.12 MHz, 40.68 MHz — limited power)
  - Research/experimental license

### 3.2 Encryption
- The canvas mentions encryption but doesn't specify an algorithm.
- **Recommendation**: AES-256-GCM for bundle encryption, with X25519 or Kyber (post-quantum) for key exchange.
- The PN sequences for DSSS/FHSS should use a **cryptographically secure PRNG** (e.g., ChaCha20-based) seeded with a shared secret.

### 3.3 FEC Code Selection
- **LDPC** (used in DVB-S2, WiFi 6) — excellent performance near Shannon limit
- **Turbo codes** (used in LTE) — slightly worse at high rates
- **Polar codes** (used in 5G) — theoretically optimal, but newer/less mature
- **Reed-Solomon** outer code — good for burst errors but not as efficient
- **Recommendation**: LDPC (from `ldpc` crate or custom) with soft-decision decoding, plus an outer CRC.

### 3.4 OFDM Parameters
- **Subcarrier spacing**: Must be tuned to expected Doppler spread (ionospheric Doppler ~1-10 Hz at HF, but can be higher during disturbances)
- **Cyclic prefix length**: Must exceed maximum multipath delay (~2-5 ms for NVIS)
- **Number of subcarriers**: Tradeoff between PAPR, latency, and frequency selectivity
- **Typical NVIS OFDM**: 64-256 subcarriers, 50-200 Hz spacing, 5 ms CP

### 3.5 DSSS Processing Gain
- Processing gain = 10·log₁₀(chip rate / symbol rate)
- To hide below noise floor, need at least 20-30 dB of processing gain
- Example: 1 kbps symbol rate, 1 Mcps chip rate → 30 dB gain
- **Tradeoff**: Higher gain = lower data rate

### 3.6 MAC Layer (Missing from Canvas)
- How do multiple Ghost Nodes share the channel?
- For spread spectrum: CDMA (different PN codes) allows simultaneous transmission
- For FHSS: Different hop patterns
- For burst transmission: Slotted ALOHA with randomized slots
- **Recommendation**: Implement CDMA-over-DSSS as primary multiple access, with FHSS for additional anti-jam. Burst slots are randomized and very short — collisions are acceptable due to redundancy.

### 3.7 Node Addressing & Discovery
- No IP addresses! Nodes must be identifiable.
- **Proposal**: Cryptographic node ID (hash of public key), similar to how Tor/i2p/Libp2p identify peers
- Discovery: Listen for beacons, or pre-shared contact schedules
- No routing tables — flooding/bouncing with TTL

---

## 4. Things Missing from the Canvas (Gap Analysis)

### 4.1 Physics & Propagation Gaps
- **Polarization**: NVIS requires **horizontal polarization** for efficient F-layer reflection. Mentioned nowhere.
- **D-layer absorption**: During daytime, the D-layer absorbs HF below ~10 MHz. NVIS is often limited to night/dawn/dusk.
- **Solar cycle**: f₀F2 varies from ~5 MHz (solar min) to ~15 MHz (solar max). System must handle this 3:1 range.
- **Geomagnetic storms**: Can completely blackout HF for hours/days. What's the fallback?
- **Faraday rotation**: Ionosphere rotates polarization of linearly polarized waves. Circular polarization may be needed, or polarization diversity.
- **Ground wave**: For short-range (< 50 km), ground wave propagation may be more reliable than NVIS. Mentioned nowhere.

### 4.2 Hardware Gaps
- **PA linearity**: Spread spectrum (especially OFDM) has high PAPR. PA must be highly linear (back-off 6-12 dB) or use predistortion.
- **T/R switching**: Must switch antenna between transmit and receive rapidly (microseconds for burst mode). PIN diode switch or RF relay required.
- **Antenna tuner**: HF antennas are narrowband. An automatic antenna tuner is essential for frequency hopping.
- **Cooling**: High-power HF transmission generates significant heat.
- **EMI/RFI shielding**: The compute platform generates RF noise that can desense the receiver.

### 4.3 DSP Gaps
- **PAPR reduction**: OFDM has high Peak-to-Average Power Ratio. Need clipping, tone reservation, or SLM (Selected Mapping).
- **Channel estimation**: Pilot pattern design for OFDM (comb, block, scattered pilots)
- **Equalization**: Zero-forcing vs MMSE equalizer per subcarrier
- **IQ sample rate**: Nyquist for a 10 MHz band = 20 MSPS minimum. With decimation to a 10 kHz channel = 2000:1 ratio. Multi-stage decimation needed (CIC + FIR).
- **Clock drift**: Even with GPSDO, residual drift accumulates. Need periodic resynchronization symbols.

### 4.4 Stealth/Counter-Surveillance Gaps
- **Traffic analysis**: Even if signal content is hidden, the mere presence of transmission patterns can be detected. Need to model background ionospheric noise and match its temporal statistics.
- **Direction finding countermeasures**: Burst transmission helps, but a network of direction finders can triangulate over time. Beamforming can help (only send to specific direction), but the side lobes still radiate.
- **Power management**: Lower power = harder to detect, but also lower SNR at receiver. Adaptive power control based on link margin would help.
- **Compromised node recovery**: If a Ghost Node is captured, how to revoke its keys without central authority?

### 4.5 Reliability Gaps
- **No acknowledgement mechanism described** (only mentioned "waiting for ACKs")
- **What if no ACKs arrive?** How many retransmissions? Exponential backoff?
- **Shard reconstruction**: Fountain codes (RaptorQ) would be ideal — receiver only needs *any* K of N shards, not specific ones.
- **Duplicate detection**: With flooding, same shard may arrive multiple times. Need content-hash-based deduplication.
- **Storage management**: DTN buffers are finite. Eviction policy? (FIFO? TTL-based? Priority?)

### 4.6 Missing Components Entirely
- **Logging & diagnostics**: How to debug a silent, stealthy radio?
- **Remote management**: How to update software/config on a deployed node?
- **Channel simulation mode**: For testing without real ionosphere — implement ITU-R ionospheric channel models (Watterson model)
- **Loopback mode**: Full self-test by looping TX → attenuator → RX
- **Metrics/monitoring**: SNR, BER, shard completion rate, spectrum occupancy logs
- **Power supply management**: For remote solar/battery deployments

---

## 5. Development Timeline Estimate

| Phase | Effort | Prerequisites |
|-------|--------|--------------|
| HAL + SDR interface | 2-4 weeks | SDR hardware acquired |
| DDC + Decimation | 2-4 weeks | HAL done |
| Costas Loop + Sync | 4-8 weeks | DDC done |
| OFDM Core | 4-8 weeks | Sync done, FFT library |
| Spread Spectrum | 4-8 weeks | OFDM or standalone DSSS |
| FEC (BICM) | 4-8 weeks | LDPC library ported |
| Protocol (Shard + DTN) | 4-6 weeks | PHY stable |
| Cognitive Radio | 4-6 weeks | Scanner + PHY done |
| Stealth / LPI | 4-8 weeks | DSSS working |
| Ionospheric Sounding | 2-4 weeks | SDR TX capable |
| Beamforming | 4-8 weeks | Multi-channel SDR |
| Integration + Field testing | 8-12 weeks | All components |
| **Total (solo developer)** | **~12-18 months** | Full-time work |

---

## 6. Key Dependencies (Libraries & Frameworks)

| Domain | Rust Crate | Purpose |
|--------|-----------|---------|
| Complex math | `num-complex` | I/Q samples |
| FFT | `rustfft` | OFDM, spectrum scanning |
| SIMD | `std::arch` / `packed_simd` | FIR filters, vector ops |
| LDPC | `ldpc` (or custom) | Forward error correction |
| Encryption | `aes-gcm`, `chacha20`, `x25519-dalek` | Bundle encryption, PN seeds |
| Serialization | `serde`, `bincode` / `postcard` | Shard/bundle serialization |
| Persistent storage | `sled` or `sqlite` | DTN bundle store |
| Async runtime | `tokio` | Concurrent SDR streaming + protocol |
| DSP | `fir`, `iir` crates or custom | Filter design |
| Plotting/Vis | `egui` + `egui_plot` | Real-time constellation, spectrum |
| SDR drivers | `soapysdr` (C binding via FFI) | Hardware abstraction |
| CLI | `clap` | Command-line interface |

---

## 7. Recommended First Milestone: "Hello Ionosphere"

A minimal viable prototype:

1. **Transmit a known DSSS sequence** from SDR at low power into a dummy load
2. **Loopback**: receive with same SDR (or second one), perform DDC, correlate DSSS code
3. **Decode a single fixed packet** (no FEC yet)
4. **Display constellation diagram and BER**

This verifies: HAL → DDC → DSSS → Sync → Basic Demodulation.

---

## 8. Risk Register

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| SDR hardware not phase-coherent for beamforming | Medium | High | Verify before purchasing; use single-channel beamforming as fallback |
| Real-time DSP too heavy for chosen CPU | High | High | Benchmark early; consider FPGA offload for DDC, FIR, FFT |
| Ionospheric conditions unpredictable | Certain | Medium | Design for worst-case; aggressive adaptive modulation |
| Legal issues with transmission | High | Critical | Obtain HAM license; use dummy load for development; research license for testing |
| Rust SDR ecosystem immature | Medium | Medium | Be prepared to write FFI bindings; consider C/C++ for critical paths |
| FCC/Bundesnetzagentur attention | Low | Medium | Keep power low during testing; use ISM bands; coordinate with local HAM community |
| Project scope too large for solo dev | High | High | Prioritize MVP; use existing SDR frameworks (GNU Radio) for prototyping before rewriting in Rust |

---

## 9. Summary: What To Build First

**Must have (MVP):**
1. SDR HAL with DMA streaming
2. DDC + Decimation
3. Costas Loop (carrier sync)
4. DSSS spread/despread
5. Basic packet format (preamble + payload)
6. DTN bundle protocol with store-and-forward
7. Ionospheric f₀F2-based frequency selection

**Should have (v1):**
8. OFDM with adaptive modulation
9. BICM with LDPC + soft decoding
10. Cognitive spectrum scanner
11. FHSS
12. Burst transmission scheduling
13. Stealth masking (cyclostationary + artificial noise)

**Nice to have (v2):**
14. Beamforming
15. Meteor scatter sync
16. Full mesh Ghost Node coordination
17. GUI dashboard
18. Channel simulation test harness

---

*Generated from ABOS.canvas analysis — a highly ambitious SDR + ionospheric mesh networking concept. The canvas is conceptually sound but light on implementation specifics. This plan fills the gaps with practical engineering considerations.*