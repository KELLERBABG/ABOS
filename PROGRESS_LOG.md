# Atmospheric Broadcast OS (ABOS) — Progress Log

> **Last Updated:** 2026-09-26
> **Overall Completion:** T4 (feature-complete & gated) — see `AUDIT_REPORT.md`
> **Build Status:** ✅ **COMPILES — 0 WARNINGS, 0 ERRORS**
> **Test Status:** ✅ **103/103 TESTS PASSING** (workspace-wide)
> **Lint Status:** ✅ `cargo clippy --workspace --all-targets -- -D warnings` clean
> **Format Status:** ✅ `cargo fmt --all -- --check` clean
> **CI:** ✅ `.github/workflows/ci.yml` (fmt + clippy + build + test + CLI smoke, ubuntu & windows)

---

## T4 CYCLE — 2026-09-26

### Tier ladder codified
`AUDIT_REPORT.md` now defines readiness tiers T1–T5 with verifiable
criteria per tier; the old unfilled `$verdict`/`$score`/`$tier` template
placeholders and the missing `SanityCheck.md` reference are gone.

### P0 correctness bugs found & fixed
While building the e2e harness, four latent bugs were found that meant the
"working" TX/RX chain could never actually roundtrip. All fixed and covered
by tests:

| Bug | Where | Fix |
|---|---|---|
| LDPC encoder produced **invalid codewords** (nonzero syndrome) — its "simplified Gaussian elimination" computed `p = Bᵀ·A·u` instead of solving `B·p = A·u` | `abos-fec/src/ldpc.rs` | Rebuilt `H = [A \| I]` systematic form; parity is `p = A·u`, syndrome zero by construction (test: 100 random payloads) |
| LDPC decoder used the **wrong LLR sign convention** (check-node math assumes positive→bit0; workspace convention is positive→bit1), converging to a valid but bit-flipped codeword | `abos-fec/src/ldpc.rs` | Convert convention only at decode boundary |
| LDPC parity placement used a deterministic `(rng+col)%m` pattern producing **correlated duplicate rows** (trapping sets) → single-bit errors uncorrectable | `abos-fec/src/ldpc.rs` | Seeded ChaCha12 uniform row placement |
| DSSS `despread` correlated every chip in a group against **one** chip — not the inverse of `spread` | `abos-phy/src/dsss.rs` | Per-chip correlation; roundtrip + wrong-seed tests added |
| Burst header: `encode_header` writes **13 bytes (52 QPSK symbols)** but `parse_burst` read **10 bytes (40)** — every payload shifted by 12 samples; sync word also hard-coded `[0;32]` vs transmitter's shared seed | `abos-phy/src/burst.rs` | Parser reads 13 bytes/52 symbols; `parse_burst(samples, sync_seed)` takes the seed |
| Shard "fountain" redundancy XORed each redundant shard **with itself** (all-zero copies), and `reconstruct_file` ignored redundant shards entirely | `abos-protocol/src/shard.rs` | Repetition copies of source shard `i % total`; two-pass reconstruction fills gaps from copies |
| `ABOSSystem::transmit` panicked on any real payload (interleaver block exceeded 512 bits) — never noticed because no test called it | `src/lib.rs` | Chain chunked per codeword; `bundle_store` now opened by default from `data_dir` |

### New: mesh / Ghost-node coordination (`abos-protocol::mesh`)
- **Discovery**: periodic beacons, peer table with last-seen expiry, flooded
  copies deduped by timestamp, never reflect own beacons.
- **ACK aggregation**: per-bundle pending tracking with **exponential
  backoff** (injected-clock `*_at` methods for deterministic tests); ACKs
  retire pending entries; aggregation lists per bundle.
- **Shard availability**: per-peer, per-file shard index sets; union view.
- **Forwarding**: `ABOSSystem::receive()` now has a *real* forwarding path
  (dedup → `increment_hop` → requeue via bounce engine → retrack for ACKs)
  replacing the commented-out TODO.

### New: `abos-tests` harness (was an empty file)
- `channel::LoopbackChannel` — seeded, reproducible loss/duplication/
  corruption/AWGN impairments for bundles and I/Q samples.
- `pipeline::Modem` — full digital TX/RX chain composed from the workspace's
  real scrambler/LDPC/interleaver/QPSK/OFDM/DSSS components;
  **byte-identical roundtrips** on clean channels, LDPC-corrected on noise.
- `tests/e2e_roundtrip.rs` — clean/noisy/DSSS/truncated/corrupted cases;
  corruption never returns wrong bytes silently.
- `tests/mesh_three_node.rs` — 3-node discovery, flooding termination, ACK
  backoff, availability tracking, dedup FIFO eviction.
- `tests/fault_injection.rs` — loss covered by redundancy, heavy loss →
  error not garbage, checksum rejection, store dedup, TTL eviction,
  pending-listing filters.

### New: `abos-cli` (was an empty file)
- clap-derive CLI producing the `abos` binary: 8 original subcommands plus
  `mesh` and `forward`; argument validation with proper exit codes
  (e.g. invalid chirp range → exit 2). Root `src/main.rs` replaced by a
  thin wrapper; `--help`, `chirp`, `mesh`, `forward` smoke-tested in CI.

### New: `abos-gui` (was an empty file)
- Headless `GuiState` (spectrum normalization, whitespace regions, peer
  table, log-tail cap, status text) — every rule unit-tested without a
  display.
- `view::dashboard_ui` — egui rendering (spectrum bars, peer grid, log
  tail) with **headless render tests** (`egui::Context::run`).

### Storage completion (`abos-storage`)
- `BundleStore`: dedup-on-insert, `evict_expired`, `list_pending`,
  `contains/len/is_empty`, `flush`. 4 unit tests.

### Pipeline & QA
- `cargo fmt --all` normalization (was 96 diffs).
- All ~36 clippy warnings fixed (not suppressed); `-D warnings` gate green.
- GitHub Actions CI on ubuntu + windows: fmt → clippy → build → test →
  CLI smoke, with cargo caching.

### Test count evolution
| Cycle | Tests |
|---|---|
| Before T4 | 43 (4 crates) |
| After T4 | **103** (11 crates) |

---

## Historical: FINAL BUILD — 2026-07-25

- 43 unit tests across root, `abos-common`, `abos-dsp`, `abos-fec`.
- Warnings fixed; `qpsk_to_bits` constellation bug fixed;
  `foF2.rs` → `fo_f2.rs` rename.

---

*This log is updated after each build step.*
