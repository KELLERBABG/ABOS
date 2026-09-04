# Atmospheric Broadcast OS (ABOS)

A software-defined radio (SDR) operating system written in Rust that enables
**covert, resilient, long-range communication** by exploiting ionospheric
propagation physics. ABOS treats the atmosphere itself as a distributed relay,
memory buffer, and stealth medium.

> **Status:** Build and tests are green. This workspace is provided "as is";
> several layers are complete and tested, while a few workspace members are
> scaffolding/placeholder crates (see [Workspace layout](#workspace-layout) and
> [Status & known gaps](#status--known-gaps) below).

---

## What it does

ABOS combines **spread-spectrum stealth**, **cognitive radio**,
**delay-tolerant networking (DTN)** and **ionospheric sounding** into one design
for decentralized mesh communication *without routing tables*. It targets
Near-Vertical Incidence Skywave (NVIS) propagation in the 2–10 MHz amateur
band, where signals reflect off the ionosphere almost straight down and reach
beyond line of sight with no fixed infrastructure.

A transmission is split into shards, FEC-encoded, DSSS-spread, stealth-masked
and delivered through DTN store-and-forward with opportunistic scheduling — see
`WHITEPAPER.md` for the full technical description and `ABOS_Building_Plan.md`
for the detailed build plan.

---

## Requirements

- A recent **Rust toolchain** (`cargo`/`rustc`, edition 2021). Verified with
  Rust 1.97 (Windows x86_64).
- Actual RF operation additionally requires SDR hardware (e.g. LimeSDR,
  HackRF, USRP) and — in most jurisdictions — an amateur-radio or experimental
  license for the NVIS/meteor bands. See the legal note in `WHITEPAPER.md`.

No system-level radio software (e.g. the on-wire SDR backend) is bundled; the
repo builds and tests fully offline.

---

## Build & test

Build the whole workspace (debug and release):

```console
cargo build                      # debug
cargo build --release --workspace
```

Run the full test suite across every crate:

```console
cargo test --workspace
```

Expected result: the workspace compiles with no errors and the tests pass
(43 unit tests across the core crates, plus each crate's own integration-test
binary and doc-tests). See [Status & known gaps](#status--known-gaps).

Run the static linter:

```console
cargo clippy --workspace
```

Clippy reports only style warnings (no errors).

---

## Quick start (CLI binary)

The command-line interface is the root `abos` binary (8 subcommands). After a
release build:

```console
target\release\abos.exe status      # Show loaded configuration
target\release\abos.exe configure   # Print current config values
target\release\abos.exe scan        # Scan the spectrum (requires SDR/stub)
target\release\abos.exe chirp       # Generate a chirp-sounder waveform
target\release\abos.exe transmit <file>
target\release\abos.exe receive
```

Configuration lives in `abos_config.json` (created automatically with defaults
if missing). RF subcommands require an actual SDR device and appropriate
licence; the pure-computation commands (`chirp`, `configure`, `status`) run
without hardware.

---

## Workspace layout

| Crate | Role | State |
|---|---|---|
| `abos` (root) | System orchestrator `ABOSSystem` + CLI binary | Implemented, tested (5) |
| `abos-hal` | SDR abstraction, DMA, GPIO, GPSDO timer | Implemented |
| `abos-dsp` | DDC, AGC, I/Q correction, Costas, Gardner, OFDM, FFT, RRC | Implemented, tested |
| `abos-phy` | DSSS, FHSS, scrambler, burst builder/parser | Implemented |
| `abos-fec` | LDPC, BICM interleaver, soft-decision LLR, CRC32 | Implemented, tested |
| `abos-protocol` | Shard split, DTN bundle, buffer-bounce, scheduler, routing | Implemented |
| `abos-cognitive` | Spectrum scanner, jammer detect, white-space, adaptive MCS | Implemented |
| `abos-iono` | Chirp sounder, f0F2, NVIS selection, meteor, MUF | Implemented |
| `abos-stealth` | Cyclostationary masking, phase noise, amp dither, burst rand | Implemented |
| `abos-storage` | Persistent DTN bundle store, system config | Implemented |
| `abos-common` | Complex math, PN/Gold codes, AES/HMAC crypto, node ID | Implemented, tested |
| `abos-cli` | Placeholder crate (empty lib) | Scaffolding only |
| `abos-gui` | Optional GUI (egui) | Stub (0%) |
| `abos-tests` | Integration/loopback harness | Placeholder crate (empty lib) |

The CLI lives in the root `abos` binary (`src/main.rs`), **not** in the
`abos-cli` crate (which is an empty placeholder).

---

## Status & known gaps

- **Green:** full workspace compiles (`cargo build`, `cargo build --release`),
  and `cargo test --workspace` passes — 43 unit tests across the root,
  `abos-common`, `abos-dsp` and `abos-fec` crates, plus integration-test
  binaries and doc-tests.
- **Honest gaps:** `abos-gui` is an empty stub, and `abos-cli` / `abos-tests`
  are empty placeholder crates. Mesh/Ghost-node coordination and the GUI
  dashboard are not implemented. These are declared scaffolding and are **not**
  yet exercised by tests.
- `cargo test` emits a few warnings (unused variables/import) in two
  integration-test files; `cargo clippy` additionally reports style warnings.
  Neither affects correctness.
- Live over-the-air behaviour and real SDR interaction were **not** verified in
  a CI environment (no radio hardware available).

---

## Documentation

- `WHITEPAPER.md` — architecture, RF chains, crypto and status/roadmap.
- `ABOS_Building_Plan.md` — detailed layer-by-layer build plan.
- `PROGRESS_LOG.md` — build/test history and per-crate completion notes.

---

## License & disclaimer

The repository declares no license file. Without an explicit license, all
rights are reserved by default; seek permission from the maintainers before
reuse. Radio operation on NVIS/meteor bands is regulated and generally
requires a licence in most jurisdictions — you are responsible for lawful use.
