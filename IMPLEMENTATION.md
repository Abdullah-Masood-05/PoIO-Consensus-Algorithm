# PoIO v0.2.0 — Implementation Reference

> **Proof of I/O (PoIO)** — A hardware-equitable consensus algorithm bounded by
> PCIe NVMe storage I/O latency rather than raw CPU/GPU computation.

---

## Table of Contents

1. [What Changed in v0.2.0](#1-what-changed-in-v020)
2. [Architecture Overview](#2-architecture-overview)
3. [Module Reference](#3-module-reference)
4. [CLI Reference](#4-cli-reference)
5. [How to Use — Step by Step](#5-how-to-use--step-by-step)
6. [Design Decisions and Protocol Guarantees](#6-design-decisions-and-protocol-guarantees)
7. [Security Mitigations](#7-security-mitigations)
8. [Performance Targets](#8-performance-targets)
9. [Dependencies](#9-dependencies)

---

## 1. What Changed in v0.2.0

### New Features

| Feature | Detail |
|---|---|
| **4 CLI subcommands** | `plot`, `mine`, `bench`, `verify` — each fully flagged |
| **Multi-threaded miner** | Rayon work-stealing pool; one `File` handle per thread — no lock contention on seek position |
| **BlockProof type** | Serializable struct containing all 128 raw 4 KiB chunks; enables asymmetric O(1)-disk verification |
| **`verify` subcommand** | Full proof validation without touching disk — any light node can run this |
| **`bench` subcommand** | Measures real NVMe random 4 KiB IOPS + latency under PoIO-exact access pattern |
| **Platform-aware direct I/O** | `FILE_FLAG_NO_BUFFERING` on Windows, `O_DIRECT` on Linux, `F_NOCACHE` on macOS |
| **Progress bars** | `indicatif` spinners and progress bars for plot generation and mining telemetry |
| **Graceful Ctrl-C** | `ctrlc` crate; all worker threads stop cleanly |
| **Skip re-generation** | Plot init checks existing file size; skips if already valid (use `--force` to override) |
| **Proof JSON export** | `mine --proof-out <file>` saves the winning proof for `verify` |
| **Coloured terminal output** | `colored` crate; headers, hash values, warnings all highlighted |

### Removed / Replaced

| Old | Replaced With |
|---|---|
| Single-threaded mine loop in `main.rs` | `core::miner::run_miner()` — Rayon parallel loop |
| Flat `Args` struct with mixed flags | Clap `Subcommand` enum with per-command flags |
| `hex_encode` in `main.rs` | `crypto::hex_encode` (shared utility) |
| `check_difficulty` in `main.rs` | `crypto::meets_difficulty` (shared, also used in verify) |
| Manual ChaCha20 calls in `main.rs` | `crypto::derive_seed` + `crypto::generate_chunk_indices` |

---

## 2. Architecture Overview

```
+------------------------------------------------------+
|                    CLI  (main.rs)                    |
|  plot --> cmd_plot     mine --> cmd_mine             |
|  bench --> cmd_bench   verify --> cmd_verify         |
+-------+----------------------+-----------------------+
        |                      |
        v                      v
+---------------+    +--------------------------------+
|  core/plot.rs |    |        core/miner.rs           |
|               |    |  Rayon thread pool             |
|  ChaCha8 PRNG |    |  Thread 0 --> File handle 0   |
|  4 MiB writes |    |  Thread 1 --> File handle 1   |
|  Progress bar |    |  Thread N --> File handle N   |
+---------------+    +----------+---------------------+
                                |  reads via
                                v
                     +---------------------+
                     |    core/disk.rs     |
                     |                     |
                     |  open_direct()      |
                     |  Windows: NO_BUFFERING
                     |  Linux:   O_DIRECT  |
                     |  macOS:   F_NOCACHE |
                     +----------+----------+
                                |  128 x 4 KiB reads
                                v
                     +---------------------+
                     |    core/crypto.rs   |
                     |                     |
                     |  derive_seed()      |
                     |  generate_indices() |
                     |  HashState (Blake3) |
                     |  meets_difficulty() |
                     |  verify_block_proof |
                     +---------------------+

                     +---------------------+
                     |    core/bench.rs    |
                     |  1024 timed reads   |
                     |  latency histogram  |
                     |  IOPS + h/s report  |
                     +---------------------+
```

### Mining Loop Data Flow (per thread, hot path)

```
nonce (u64)
    |
    v  Blake3(header || nonce_le)
seed [u8; 32]
    |
    v  ChaCha8Rng::from_seed(seed) -> 128 x next_u64() % num_chunks
indices [u64; 128]
    |
    v  seek(index x 4096)  ->  read_exact(&mut [u8; 4096])   <- PCIe bottleneck
128 x chunk [u8; 4096]
    |
    v  Blake3 streaming update x 128  (no heap allocation)
final_hash [u8; 32]
    |
    v  leading zero bits >= difficulty?
   YES --> BlockProof { nonce, indices, chunks, final_hash }
    NO --> nonce += threads x SEGMENT  ->  repeat
```

---

## 3. Module Reference

### `src/core/crypto.rs`

| Symbol | Purpose |
|---|---|
| `HashState` | Thin `blake3::Hasher` wrapper; stays on stack per mining attempt |
| `derive_seed(header, nonce)` | `Blake3(header || nonce_le)` returns `[u8; 32]` |
| `generate_chunk_indices(seed, num_chunks)` | ChaCha8 PRNG returns `[u64; 128]` deterministically |
| `meets_difficulty(hash, bits)` | Check leading zero bits in constant time |
| `verify_block_proof(proof, difficulty)` | Full asymmetric verification without disk I/O |
| `hex_encode(bytes)` | Pre-allocated lowercase hex string |

### `src/core/disk.rs`

| Symbol | Purpose |
|---|---|
| `open_direct(path)` | OS-specific cache-bypass file open |
| `read_chunk_at_offset(file, offset, buf)` | `seek` + `read_exact` into a caller-supplied buffer |

`FILE_FLAG_NO_BUFFERING` on Windows requires:
- I/O buffer aligned to 512 bytes — our `[u8; 4096]` stack array satisfies this
- Read sizes that are multiples of the physical sector size (4096 is correct)
- File offsets that are multiples of the sector size (`index x 4096` is correct)

### `src/core/plot.rs`

| Symbol | Purpose |
|---|---|
| `plot_is_valid(path, size)` | Check if existing file matches expected byte count |
| `initialize_plot(path, size, seed, force)` | Generate deterministic high-entropy plot data |

The plot uses `ChaCha8Rng` so every byte of the file is unpredictable — compression attacks yield zero space savings.

### `src/core/miner.rs`

| Symbol | Purpose |
|---|---|
| `CHUNK_SIZE = 4096` | Protocol-mandated read unit (bytes) |
| `REQUIRED_READS = 128` | Protocol-mandated reads per hash attempt |
| `BlockProof` | Self-contained verifiable proof struct (serde JSON) |
| `MiningStats` | Shared atomic counters for live telemetry |
| `MinerConfig` | Configuration bundle passed to `run_miner` |
| `run_miner(config, stats, stop)` | Launch Rayon pool; return first winning `BlockProof` |
| `format_duration(d)` | Human-readable elapsed time string |

**Thread nonce partitioning:**
Each thread starts at `nonce_0 + thread_id x SEGMENT` and advances by
`threads x SEGMENT` per iteration, covering the nonce space without overlap.
`SEGMENT = 1024` keeps cache-line behaviour clean.

### `src/core/bench.rs`

| Symbol | Purpose |
|---|---|
| `run_benchmark(path, num_chunks)` | 1024 timed random reads returning `BenchResult` |
| `print_report(result)` | Formatted table with protocol reference targets |

### `src/main.rs`

Four subcommand handlers: `cmd_plot`, `cmd_mine`, `cmd_bench`, `cmd_verify`.
A background display thread updates the live mining telemetry spinner every 250 ms
without blocking the Rayon worker threads.

---

## 4. CLI Reference

### `poio plot` — Generate a plot file

```
poio plot [OPTIONS]

Options:
  -p, --path <PATH>      Output path              [default: ./poio.plot]
  -s, --size <BYTES>     Plot size in bytes       [default: 52428800 = 50 MB]
  -g, --genesis <HEX>    32-byte genesis seed (64 hex chars)
                         [default: 000...000 (testnet)]
  -f, --force            Overwrite existing plot even if valid
  -h, --help             Print help
```

**Size guidelines:**

| Size | Flag value | Use case |
|---|---|---|
| 50 MB | `52428800` | Demo / CI |
| 512 MB | `536870912` | Laptop testing |
| 1 GB | `1073741824` | Sustained benchmark |
| 100 GB+ | — | Production (future) |

---

### `poio mine` — Run the mining loop

```
poio mine [OPTIONS]

Options:
  -p, --path <PATH>          Plot file to mine against  [default: ./poio.plot]
  -h, --header <STR>         Block header string        [default: poio_genesis_block_v1]
  -n, --nonce <N>            Starting nonce             [default: 0]
  -d, --difficulty <BITS>    Leading zero bits required [default: 4]
  -t, --threads <N>          Worker threads             [default: logical CPU count]
  -m, --max-attempts <N>     Stop after N hash attempts
  -o, --proof-out <PATH>     Save winning proof to JSON file
      --help                 Print help
```

**Difficulty reference:**

| `--difficulty` | Expected attempts | Use case |
|---|---|---|
| `4` | ~16 | Demo / fast test |
| `8` | ~256 | Quick benchmark |
| `12` | ~4,096 | Integration test |
| `16` | ~65,536 | Realistic simulation |
| `20` | ~1,048,576 | Near-production |

---

### `poio bench` — Benchmark NVMe throughput

```
poio bench [OPTIONS]

Options:
  -p, --path <PATH>    Plot file to benchmark against  [default: ./poio.plot]
  -s, --size <BYTES>   Expected plot size in bytes     [default: 52428800]
      --help           Print help
```

Performs **1,024 random 4 KiB reads** using the same direct I/O path as the
miner, then prints mean/median latency, IOPS, and estimated hashes/sec with
a comparison against protocol reference targets.

---

### `poio verify` — Verify a block proof

```
poio verify [OPTIONS]

Options:
  -p, --proof <PATH>      Path to proof JSON file (from mine --proof-out)
  -d, --difficulty <BITS> Difficulty level to verify against  [default: 4]
      --help              Print help
```

**No disk access required.** Re-derives the 128 chunk indices from
`(header, nonce)` and re-hashes the attached chunks to confirm the proof.
This is what any light node on the network would run.

---

## 5. How to Use — Step by Step

### Prerequisites

```powershell
rustc --version   # 1.75+
cargo --version
```

### Step 1 — Build the release binary

```powershell
cargo build --release
# Binary: target\release\poio.exe
```

### Step 2 — Generate a plot file (50 MB demo)

```powershell
cargo run --release -- plot --size 52428800 --path .\poio.plot
```

Expected output:
```
  Generating plot: 52428800 bytes -> "./poio.plot"
  [00:00:01] ████████████████████████████████████████████ 50.0 MiB / 50.0 MiB
  Plot ready in 1.234s
```

### Step 3 — Run the miner (easy difficulty for demo)

```powershell
cargo run --release -- mine --difficulty 4 --threads 4
```

Expected output:
```
  Plot     : "./poio.plot"  (12800 chunks)
  Difficulty: 4 leading zero bits
  Threads  : 4

  Attempts:     47  |  43.21 h/s  |  5531 IOPS

  BLOCK found!
  Nonce      : 23
  Hash       : 0d3f8a2b...
  Elapsed    : 0.541s
  Attempts   : 47
  Throughput : 43.21 h/s  |  5531 IOPS
```

### Step 4 — Save and verify the proof

```powershell
# Mine and export proof JSON
cargo run --release -- mine --difficulty 4 --proof-out .\proof.json

# Verify without touching the plot file
cargo run --release -- verify --proof .\proof.json --difficulty 4
```

Expected verification output:
```
  Proof is VALID - all 128 chunk indices match deterministic derivation
  Hash satisfies difficulty 4
```

### Step 5 — Benchmark your NVMe drive

```powershell
cargo run --release -- bench --path .\poio.plot --size 52428800
```

Expected output:
```
  Total Reads                 1024    4 KiB reads
  Elapsed                    0.110 s  wall clock
  IOPS                        9309    reads/sec
  Mean Latency               107.4 us per 4 KiB read
  Median Latency             104.2 us per 4 KiB read
  Est. Hashes/sec             72.73   at 128 reads/hash

  Your device: GOOD - consumer NVMe within protocol target
```

---

## 6. Design Decisions and Protocol Guarantees

### Why 128 reads per hash?

128 x 4 KiB = 512 KiB of data touched per hash attempt.
At 100 µs median NVMe latency this gives ~78 h/s theoretical maximum per drive.
128 was chosen as the minimum that makes a RAM-disk attack economically prohibitive
at 50 MB+ plot sizes while keeping demo throughput human-observable.

### Why ChaCha8 and not ChaCha20?

ChaCha8 (8 rounds vs 20) is cryptographically sufficient for PRNG offset derivation
and is approximately 2.5x faster. The security requirement is unpredictability of
indices given the seed, not full cryptanalytic resistance of the PRNG itself.

### Why Blake3 and not SHA-256?

Blake3 is 3-10x faster than SHA-256 on modern CPUs with SIMD support.
Since PoIO is I/O-bound rather than CPU-bound, this keeps CPU overhead negligible
and preserves the I/O bottleneck as the sole network constraint.

### Thread-local file handles

Each Rayon worker opens its own `File` handle. On Windows, a shared handle would
require a mutex around every seek+read pair (defeating parallelism). Thread-local
handles allow all workers to issue simultaneous `ReadFile` calls without any
synchronisation overhead.

### Asymmetric verification

Mining is I/O-hard: 128 physical reads required per attempt.
Verification is CPU-only: re-derives the 128 expected indices from `(header, nonce)`
via ChaCha8, then rehashes the 128 attached chunk payloads with Blake3.
This takes under 1 ms on any modern CPU, enabling lightweight node participation.

---

## 7. Security Mitigations

| Attack | Mitigation |
|---|---|
| **RAM-disk caching** | Plot files scale to sizes cost-prohibitive in DRAM. A 1 TB plot requires approximately $10k+ of DDR5 ECC RAM. 50 MB demo plots are intentionally small and this is documented. |
| **OS page cache** | `FILE_FLAG_NO_BUFFERING` on Windows and `O_DIRECT` on Linux bypass the kernel page cache, forcing all reads through the physical PCIe bus. |
| **Compression of plot** | `ChaCha8Rng` output has maximum entropy; gzip/zstd achieve 0% compression ratio on plot files. |
| **On-the-fly regeneration** | Chunk indices are derived from `Blake3(header || nonce)` using the current block header, unknown until the challenge arrives. Pre-computation is impossible. |
| **ASIC acceleration** | The PCIe interface is a commodity hardware standard. No ASIC can bypass the physical NAND flash access latency of 60-110 µs. |
| **Proof forgery** | `verify_block_proof` re-derives all 128 indices from `(header, nonce)` independently and recomputes the final hash. Any tampered chunk causes an immediate hash mismatch. |

---

## 8. Performance Targets

| Metric | Consumer NVMe | Enterprise NVMe |
|---|---|---|
| Random 4 KiB latency | 90-110 µs | 60-80 µs |
| Achievable IOPS | ~9,000-11,000 | ~12,000-16,000 |
| Est. hashes/sec per drive | 50-80 h/s | 80-120 h/s |
| Plot generation speed | ~500 MB/s | ~1,000 MB/s |

The **1.33x** performance ratio between consumer and enterprise drives is the
core PoIO equity guarantee — compared to a **100-1,000x** ratio in traditional PoW.

---

## 9. Dependencies

| Crate | Version | Role |
|---|---|---|
| `blake3` | 1.5.1 | Fast cryptographic hashing for seed and final hash |
| `rand_chacha` | 0.3.1 | ChaCha8 PRNG for deterministic chunk index generation |
| `rand_core` | 0.6.4 | Core PRNG trait bounds |
| `clap` | 4.5 | CLI subcommand parsing with derive macros |
| `rayon` | 1.10 | Work-stealing parallel thread pool for mining |
| `indicatif` | 0.17 | Progress bars and terminal spinners |
| `colored` | 2.1 | Coloured terminal output |
| `ctrlc` | 3.4 | Cross-platform Ctrl-C handler for graceful shutdown |
| `serde` + `serde_json` | 1 | BlockProof serialisation to and from JSON |

---

*PoIO v0.2.0 — Academic Research Project*
*Bazil Suhail (Bscs22072) · Abdullah Masood (Bscs22054) · Ebad Junaid (Bscs22046)*
