use crate::dma::DMABuffer;
use abos_common::error::{Error, Result};
use abos_common::types::SDRConfig;
use num_complex::Complex64;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub trait SDRDevice: Send {
    fn configure(&mut self, config: SDRConfig) -> Result<()>;
    fn start_stream(&mut self) -> Result<()>;
    fn stop_stream(&mut self) -> Result<()>;
    fn read_samples(&mut self, buffer: &mut [Complex64]) -> Result<usize>;
    fn write_samples(&mut self, samples: &[Complex64]) -> Result<usize>;
}

pub struct USRPSDR {
    config: Option<SDRConfig>,
    running: bool,
    buffer: DMABuffer,
}

impl Default for USRPSDR {
    fn default() -> Self {
        Self::new()
    }
}

impl USRPSDR {
    pub fn new() -> Self {
        Self {
            config: None,
            running: false,
            buffer: DMABuffer::new(4096),
        }
    }
}

impl SDRDevice for USRPSDR {
    fn configure(&mut self, config: SDRConfig) -> Result<()> {
        self.config = Some(config);
        Ok(())
    }
    fn start_stream(&mut self) -> Result<()> {
        if self.config.is_none() {
            return Err(Error::ConfigError("USRP not configured".into()));
        }
        self.running = true;
        Ok(())
    }
    fn stop_stream(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }
    fn read_samples(&mut self, buffer: &mut [Complex64]) -> Result<usize> {
        if !self.running {
            return Err(Error::SdrError("USRP stream not started".into()));
        }
        let n = buffer.len().min(1024);
        buffer[..n].fill(Complex64::new(0.0, 0.0));
        Ok(n)
    }
    fn write_samples(&mut self, samples: &[Complex64]) -> Result<usize> {
        if !self.running {
            return Err(Error::SdrError("USRP stream not started".into()));
        }
        self.buffer.write_samples(samples);
        Ok(samples.len())
    }
}

pub struct LimeSDR {
    config: Option<SDRConfig>,
    running: bool,
    buffer: DMABuffer,
}

impl Default for LimeSDR {
    fn default() -> Self {
        Self::new()
    }
}

impl LimeSDR {
    pub fn new() -> Self {
        Self {
            config: None,
            running: false,
            buffer: DMABuffer::new(4096),
        }
    }
}

impl SDRDevice for LimeSDR {
    fn configure(&mut self, config: SDRConfig) -> Result<()> {
        self.config = Some(config);
        Ok(())
    }
    fn start_stream(&mut self) -> Result<()> {
        if self.config.is_none() {
            return Err(Error::ConfigError("LimeSDR not configured".into()));
        }
        self.running = true;
        Ok(())
    }
    fn stop_stream(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }
    fn read_samples(&mut self, buffer: &mut [Complex64]) -> Result<usize> {
        if !self.running {
            return Err(Error::SdrError("LimeSDR stream not started".into()));
        }
        let n = buffer.len().min(1024);
        buffer[..n].fill(Complex64::new(0.0, 0.0));
        Ok(n)
    }
    fn write_samples(&mut self, samples: &[Complex64]) -> Result<usize> {
        if !self.running {
            return Err(Error::SdrError("LimeSDR stream not started".into()));
        }
        self.buffer.write_samples(samples);
        Ok(samples.len())
    }
}

pub struct HackRFSDR {
    config: Option<SDRConfig>,
    running: bool,
    buffer: DMABuffer,
}

impl Default for HackRFSDR {
    fn default() -> Self {
        Self::new()
    }
}

impl HackRFSDR {
    pub fn new() -> Self {
        Self {
            config: None,
            running: false,
            buffer: DMABuffer::new(4096),
        }
    }
}

impl SDRDevice for HackRFSDR {
    fn configure(&mut self, config: SDRConfig) -> Result<()> {
        self.config = Some(config);
        Ok(())
    }
    fn start_stream(&mut self) -> Result<()> {
        if self.config.is_none() {
            return Err(Error::ConfigError("HackRF not configured".into()));
        }
        self.running = true;
        Ok(())
    }
    fn stop_stream(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }
    fn read_samples(&mut self, buffer: &mut [Complex64]) -> Result<usize> {
        if !self.running {
            return Err(Error::SdrError("HackRF stream not started".into()));
        }
        let n = buffer.len().min(1024);
        buffer[..n].fill(Complex64::new(0.0, 0.0));
        Ok(n)
    }
    fn write_samples(&mut self, samples: &[Complex64]) -> Result<usize> {
        if !self.running {
            return Err(Error::SdrError("HackRF stream not started".into()));
        }
        self.buffer.write_samples(samples);
        Ok(samples.len())
    }
}

/// In-memory loopback SDR device.
///
/// In standalone mode, transmitted samples are immediately available for reading.
/// In paired mode ([`LoopbackSDR::pair`]), transmission on device A feeds reception on device B,
/// modeling a clean over-the-air link between two nodes.
pub struct LoopbackSDR {
    config: Option<SDRConfig>,
    running: bool,
    tx_queue: Arc<Mutex<VecDeque<Complex64>>>,
    rx_queue: Arc<Mutex<VecDeque<Complex64>>>,
}

impl Default for LoopbackSDR {
    fn default() -> Self {
        Self::new()
    }
}

impl LoopbackSDR {
    /// Create a standalone loopback SDR where TX queues into its own RX.
    pub fn new() -> Self {
        let q = Arc::new(Mutex::new(VecDeque::new()));
        Self {
            config: None,
            running: false,
            tx_queue: q.clone(),
            rx_queue: q,
        }
    }

    /// Create a cross-connected pair of loopback SDR devices.
    pub fn pair() -> (Self, Self) {
        let q1_to_2 = Arc::new(Mutex::new(VecDeque::new()));
        let q2_to_1 = Arc::new(Mutex::new(VecDeque::new()));
        let dev1 = Self {
            config: None,
            running: false,
            tx_queue: q1_to_2.clone(),
            rx_queue: q2_to_1.clone(),
        };
        let dev2 = Self {
            config: None,
            running: false,
            tx_queue: q2_to_1,
            rx_queue: q1_to_2,
        };
        (dev1, dev2)
    }

    /// Number of samples currently queued in the receive buffer.
    pub fn pending_rx_samples(&self) -> usize {
        self.rx_queue.lock().map(|q| q.len()).unwrap_or(0)
    }
}

impl SDRDevice for LoopbackSDR {
    fn configure(&mut self, config: SDRConfig) -> Result<()> {
        self.config = Some(config);
        Ok(())
    }

    fn start_stream(&mut self) -> Result<()> {
        if self.config.is_none() {
            return Err(Error::ConfigError("LoopbackSDR not configured".into()));
        }
        self.running = true;
        Ok(())
    }

    fn stop_stream(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }

    fn read_samples(&mut self, buffer: &mut [Complex64]) -> Result<usize> {
        if !self.running {
            return Err(Error::SdrError("LoopbackSDR stream not started".into()));
        }
        let mut q = self.rx_queue.lock().unwrap();
        let count = buffer.len().min(q.len());
        for item in buffer.iter_mut().take(count) {
            *item = q.pop_front().unwrap();
        }
        Ok(count)
    }

    fn write_samples(&mut self, samples: &[Complex64]) -> Result<usize> {
        if !self.running {
            return Err(Error::SdrError("LoopbackSDR stream not started".into()));
        }
        let mut q = self.tx_queue.lock().unwrap();
        q.extend(samples.iter().copied());
        Ok(samples.len())
    }
}

/// File-based SDR backend compatible with SigMF / GNU Radio (.cf32 raw interleaved float32).
pub struct FileSDR {
    path: std::path::PathBuf,
    is_tx: bool,
    config: Option<SDRConfig>,
    running: bool,
    tx_file: Option<std::fs::File>,
    rx_samples: VecDeque<Complex64>,
}

impl FileSDR {
    pub fn new(path: impl Into<std::path::PathBuf>, is_tx: bool) -> Self {
        Self {
            path: path.into(),
            is_tx,
            config: None,
            running: false,
            tx_file: None,
            rx_samples: VecDeque::new(),
        }
    }
}

impl SDRDevice for FileSDR {
    fn configure(&mut self, config: SDRConfig) -> Result<()> {
        self.config = Some(config);
        Ok(())
    }

    fn start_stream(&mut self) -> Result<()> {
        if self.config.is_none() {
            return Err(Error::ConfigError("FileSDR not configured".into()));
        }
        if self.is_tx {
            let f = std::fs::File::create(&self.path).map_err(Error::IoError)?;
            self.tx_file = Some(f);
        } else {
            let bytes = std::fs::read(&self.path).map_err(Error::IoError)?;
            let mut q = VecDeque::with_capacity(bytes.len() / 8);
            #[allow(clippy::chunks_exact_to_as_chunks)]
            for chunk in bytes.chunks_exact(8) {
                let re = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as f64;
                let im = f32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]) as f64;
                q.push_back(Complex64::new(re, im));
            }
            self.rx_samples = q;
        }
        self.running = true;
        Ok(())
    }

    fn stop_stream(&mut self) -> Result<()> {
        self.running = false;
        self.tx_file = None;
        self.rx_samples.clear();
        Ok(())
    }

    fn read_samples(&mut self, buffer: &mut [Complex64]) -> Result<usize> {
        if !self.running {
            return Err(Error::SdrError("FileSDR stream not started".into()));
        }
        let count = buffer.len().min(self.rx_samples.len());
        for item in buffer.iter_mut().take(count) {
            *item = self.rx_samples.pop_front().unwrap();
        }
        Ok(count)
    }

    fn write_samples(&mut self, samples: &[Complex64]) -> Result<usize> {
        if !self.running {
            return Err(Error::SdrError("FileSDR stream not started".into()));
        }
        if let Some(ref mut f) = self.tx_file {
            use std::io::Write;
            let mut buf = Vec::with_capacity(samples.len() * 8);
            for s in samples {
                buf.extend_from_slice(&(s.re as f32).to_le_bytes());
                buf.extend_from_slice(&(s.im as f32).to_le_bytes());
            }
            f.write_all(&buf).map_err(Error::IoError)?;
        }

        Ok(samples.len())
    }
}

pub enum SDRType {
    USRP,
    LimeSDR,
    HackRF,
    Loopback,
    File {
        path: std::path::PathBuf,
        is_tx: bool,
    },
}

pub fn create_sdr(device_type: SDRType) -> Box<dyn SDRDevice> {
    match device_type {
        SDRType::USRP => Box::new(USRPSDR::new()),
        SDRType::LimeSDR => Box::new(LimeSDR::new()),
        SDRType::HackRF => Box::new(HackRFSDR::new()),
        SDRType::Loopback => Box::new(LoopbackSDR::new()),
        SDRType::File { path, is_tx } => Box::new(FileSDR::new(path, is_tx)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_pair_roundtrip() {
        let (mut a, mut b) = LoopbackSDR::pair();
        let config = SDRConfig {
            center_frequency: 7_100_000,
            sample_rate: 1_000_000.0,
            gain: 30.0,
            bandwidth: 12_000.0,
            antenna_port: 0,
        };
        a.configure(config.clone()).unwrap();
        b.configure(config).unwrap();
        a.start_stream().unwrap();
        b.start_stream().unwrap();

        let tx_data = vec![Complex64::new(1.0, 0.5), Complex64::new(-0.5, 0.25)];
        a.write_samples(&tx_data).unwrap();

        let mut rx_buf = vec![Complex64::default(); 10];
        let n = b.read_samples(&mut rx_buf).unwrap();
        assert_eq!(n, 2);
        assert_eq!(&rx_buf[..2], &tx_data[..]);
    }

    #[test]
    fn file_sdr_roundtrip() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join(format!("test_cf32_{}.raw", std::process::id()));

        let mut tx = FileSDR::new(&path, true);
        let mut rx = FileSDR::new(&path, false);
        let config = SDRConfig {
            center_frequency: 7_100_000,
            sample_rate: 1_000_000.0,
            gain: 30.0,
            bandwidth: 12_000.0,
            antenna_port: 0,
        };
        tx.configure(config.clone()).unwrap();
        rx.configure(config).unwrap();

        tx.start_stream().unwrap();
        let samples = vec![Complex64::new(0.707, 0.707), Complex64::new(-0.707, 0.707)];
        tx.write_samples(&samples).unwrap();
        tx.stop_stream().unwrap();

        rx.start_stream().unwrap();
        let mut read_buf = vec![Complex64::default(); 4];
        let n = rx.read_samples(&mut read_buf).unwrap();
        rx.stop_stream().unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(n, 2);
        assert!((read_buf[0].re - 0.707).abs() < 1e-4);
        assert!((read_buf[0].im - 0.707).abs() < 1e-4);
    }
}
