//! # Atmospheric Broadcast OS (ABOS) — Command-Line Interface
//!
//! Usage: `abos [subcommand] [options]`
//!
//! Subcommands:
//! - `start`       Start the ABOS system
//! - `stop`        Stop the ABOS system
//! - `transmit`    Transmit a file
//! - `receive`     Receive and decode signals
//! - `scan`        Scan the spectrum
//! - `status`      Display system status
//! - `configure`   View/edit configuration
//! - `chirp`       Generate a chirp sounder waveform

use abos::ABOSSystem;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage(&args[0]);
        return;
    }

    let command = &args[1];

    match command.as_str() {
        "start" => cmd_start().await,
        "stop" => cmd_stop().await,
        "transmit" => {
            if args.len() < 3 {
                eprintln!("Usage: {} transmit <file>", args[0]);
                return;
            }
            cmd_transmit(&args[2]).await;
        }
        "receive" => cmd_receive().await,
        "scan" => cmd_scan().await,
        "status" => cmd_status().await,
        "configure" => cmd_configure().await,
        "chirp" => cmd_chirp().await,
        _ => {
            eprintln!("Unknown command: {}", command);
            print_usage(&args[0]);
        }
    }
}

fn print_usage(name: &str) {
    eprintln!("Atmospheric Broadcast OS (ABOS)");
    eprintln!();
    eprintln!("Usage: {} <command> [options]", name);
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  start              Start the ABOS system");
    eprintln!("  stop               Stop the ABOS system");
    eprintln!("  transmit <file>    Transmit a file");
    eprintln!("  receive            Listen and decode");
    eprintln!("  scan               Scan spectrum");
    eprintln!("  status             System status");
    eprintln!("  configure          Show configuration");
    eprintln!("  chirp              Generate chirp sounder");
}

async fn cmd_start() {
    println!("[ABOS] Starting system...");
    match ABOSSystem::new().await {
        Ok(mut system) => {
            if let Err(e) = system.start().await {
                eprintln!("[ABOS] Failed to start: {}", e);
                return;
            }
            println!("[ABOS] System started.");
            println!("[ABOS] Node ID: {}", hex::encode(&system.node_id()));
            println!("[ABOS] Center Frequency: {} Hz", system.config().center_frequency);
            println!("[ABOS] Sample Rate: {} sps", system.config().sample_rate);
            println!("[ABOS] Bandwidth: {} Hz", system.config().bandwidth);
            println!("[ABOS] Default MCS: {:?}", system.config().default_mcs);
            println!("[ABOS] Press Ctrl+C to stop.");
            tokio::signal::ctrl_c().await.ok();
            let _ = system.stop().await;
            println!("[ABOS] System stopped.");
        }
        Err(e) => {
            eprintln!("[ABOS] Failed to initialize: {}", e);
        }
    }
}

async fn cmd_stop() {
    println!("[ABOS] Stopping system...");
    let system = ABOSSystem::new().await;
    if let Ok(mut system) = system {
        let _ = system.stop().await;
        println!("[ABOS] System stopped.");
    } else {
        eprintln!("[ABOS] System not running.");
    }
}

async fn cmd_transmit(path: &str) {
    println!("[ABOS] Loading file: {}", path);
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[ABOS] Failed to read file: {}", e);
            return;
        }
    };
    println!("[ABOS] File size: {} bytes", data.len());

    match ABOSSystem::new().await {
        Ok(mut system) => {
            system.init_dsss(64);
            if let Err(e) = system.start().await {
                eprintln!("[ABOS] Failed to start: {}", e);
                return;
            }
            println!("[ABOS] Transmitting...");
            match system.transmit(&data).await {
                Ok(_) => println!("[ABOS] Transmission complete! ({} shards sent)", 
                    data.len() / 1024 + 1),
                Err(e) => eprintln!("[ABOS] Transmission failed: {}", e),
            }
            let _ = system.stop().await;
        }
        Err(e) => eprintln!("[ABOS] System init failed: {}", e),
    }
}

async fn cmd_receive() {
    println!("[ABOS] Listening for signals...");
    match ABOSSystem::new().await {
        Ok(mut system) => {
            system.init_dsss(64);
            if let Err(e) = system.start().await {
                eprintln!("[ABOS] Failed to start: {}", e);
                return;
            }
            println!("[ABOS] Waiting for bursts...");
            let mut buffer = vec![num_complex::Complex64::new(0.0, 0.0); 4096];
            for _ in 0..10 {
                match system.receive(&mut buffer).await {
                    Ok(data) => {
                        println!("[ABOS] Received {} bytes", data.len());
                        let path = format!("received_{}.bin", 
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs());
                        if std::fs::write(&path, &data).is_ok() {
                            println!("[ABOS] Saved to {}", path);
                        }
                        break;
                    }
                    Err(abos_common::error::Error::SyncLost) => {}
                    Err(e) => {
                        eprintln!("[ABOS] Error: {}", e);
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
            let _ = system.stop().await;
        }
        Err(e) => eprintln!("[ABOS] System init failed: {}", e),
    }
}

async fn cmd_scan() {
    println!("[ABOS] Scanning spectrum...");
    match ABOSSystem::new().await {
        Ok(mut system) => {
            if let Err(e) = system.start().await {
                eprintln!("[ABOS] Failed to start: {}", e);
                return;
            }
            let mut buffer = vec![num_complex::Complex64::new(0.0, 0.0); 2048];
            let _ = system.receive(&mut buffer).await;
            
            let spectrum = system.scan_spectrum(&buffer);
            let noise_floor = spectrum.iter().sum::<f64>() / spectrum.len() as f64;
            let whitespace = system.find_whitespace(&spectrum, noise_floor * 1.5);
            
            println!("[ABOS] Spectrum scan complete ({} bins)", spectrum.len());
            println!("[ABOS] Noise floor: {:.2}", noise_floor);
            println!("[ABOS] White space regions: {}", whitespace.len());
            for (i, (start, end)) in whitespace.iter().enumerate().take(10) {
                println!("[ABOS]   Region {}: bins {}-{}", i, start, end);
            }
            
            let jammers = system.detect_interferers(&spectrum, noise_floor * 5.0);
            if !jammers.is_empty() {
                println!("[ABOS] Potential interferers at bins: {:?}", &jammers[..jammers.len().min(10)]);
            }
            
            let _ = system.stop().await;
        }
        Err(e) => eprintln!("[ABOS] System init failed: {}", e),
    }
}

async fn cmd_status() {
    println!("[ABOS] System Status");
    println!("===================");
    match ABOSSystem::new().await {
        Ok(system) => {
            println!("Node ID:              {}", hex::encode(&system.node_id()));
            println!("Running:              {}", system.is_running());
            println!("Center Frequency:     {} Hz", system.config().center_frequency);
            println!("Sample Rate:          {} sps", system.config().sample_rate);
            println!("TX Gain:              {} dB", system.config().tx_gain);
            println!("RX Gain:              {} dB", system.config().rx_gain);
            println!("Bandwidth:            {} Hz", system.config().bandwidth);
            println!("Default MCS:          {:?}", system.config().default_mcs);
            println!("Data Directory:       {}", system.config().data_dir.display());
            println!("Log Level:            {}", system.config().log_level);
        }
        Err(e) => eprintln!("[ABOS] Status check failed: {}", e),
    }
}

async fn cmd_configure() {
    println!("[ABOS] Current Configuration:");
    match ABOSSystem::new().await {
        Ok(system) => {
            let config = system.config();
            println!("  center_frequency: {} Hz", config.center_frequency);
            println!("  sample_rate: {} sps", config.sample_rate);
            println!("  tx_gain: {}", config.tx_gain);
            println!("  rx_gain: {}", config.rx_gain);
            println!("  bandwidth: {}", config.bandwidth);
            println!("  default_mcs: {:?}", config.default_mcs);
            println!("  data_dir: {}", config.data_dir.display());
            println!("  log_level: {}", config.log_level);
        }
        Err(e) => eprintln!("[ABOS] Config load failed: {}", e),
    }
}

async fn cmd_chirp() {
    println!("[ABOS] Generating chirp sounder waveform...");
    let chirp = ABOSSystem::generate_chirp(3.0e6, 10.0e6, 0.1);
    println!("[ABOS] Chirp generated: {} samples ({} ms)", 
        chirp.len(), (chirp.len() as f64 / 1_000_000.0 * 1000.0) as u64);
}

/// Simple hex encoding helper (avoids pulling in the `hex` crate)
mod hex {
    pub fn encode(bytes: &[u8; 32]) -> String {
        const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";
        let mut result = String::with_capacity(64);
        for &b in bytes.iter() {
            result.push(HEX_CHARS[(b >> 4) as usize] as char);
            result.push(HEX_CHARS[(b & 0x0f) as usize] as char);
        }
        result
    }
}