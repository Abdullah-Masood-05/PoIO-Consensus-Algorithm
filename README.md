# Proof of I/O (PoIO): A Read-Bound Hashing Algorithm for Hardware-Equitable Consensus

## Overview

Proof of I/O (PoIO) is an ASIC-resistant, hardware-equitable consensus algorithm that replaces traditional computational bottlenecks (CPU/GPU hashing) with storage I/O bandwidth limitations. By tethering mining capability to consumer storage interfaces (PCIe bandwidth), PoIO prevents the centralization of mining power in specialized hardware.

## Problem Statement

Traditional Proof-of-Work (PoW) protocols suffer from fundamental hardware centralization issues:

| Issue | Impact | Current Solutions |
|-------|--------|-------------------|
| **ASIC Dominance** | Wealthy entities deploy specialized hardware, making consumer-grade mining economically unviable | Limited effectiveness; ASICs continue to advance |
| **DRAM Bottleneck** | Memory-hard functions (Argon2, Ethash) bottleneck at DRAM bandwidth limits (~tens of GB/s), still exploitable by high-end hardware | Argon2 requires large DRAM allocation but remains vulnerable |
| **Static Verification** | Proof of Space relies on static storage verification, allowing high-speed CPUs to regenerate proofs without actual disk reads | Chia's approach lacks temporal I/O dependency |

**PoIO Solution:** By enforcing 128 concurrent random I/O reads per hash attempt, the bottleneck remains at the hardware interface (PCIe bus), ensuring that mining capability cannot be arbitrarily scaled using custom computing chips.

## Core Algorithm

### Phase A: Plot Generation (One-Time Setup)

The plot file is initialized deterministically from a genesis seed:

1. Obtain the genesis block hash as the global seed
2. Initialize ChaCha20 PRNG with the seed
3. Stream bytes sequentially to generate plot data
4. Write T total bytes (default: 50 MB for testing) partitioned into N = T/4096 chunks of 4 KB each
5. Compute Merkle tree over all chunk hashes for verification

### Phase B: Mining Loop

For each block header and nonce:

1. **Seed Computation:** Compute seed = Blake3(BlockHeader || Nonce)
2. **Offset Generation:** Use ChaCha20 PRNG to generate 128 pseudo-random chunk offsets
   - Formula: O_i = (PRNG.next_u64() mod N) × 4096
3. **Disk I/O Reads:** Read 4 KB chunks from plot file at each offset (critical bottleneck)
4. **Final Hash:** Compute Blake3 hash over all read chunks
5. **Difficulty Check:** Compare final hash against target difficulty
6. **Loop:** If hash doesn't meet difficulty, increment nonce and repeat

### Architecture Flow

```
Network Challenge/Target
         |
         v
Compute Seed = Blake3(BlockHeader || Nonce)
         |
         v
ChaCha20 PRNG -> 128 Random Offsets
         |
         v
OS Disk I/O Controller (Bottleneck)
         |
         v
Physical PCIe Bus (Hardware Constraint)
         |
         v
NVMe SSD: Read 128 x 4 KB Chunks
         |
         v
CPU: FinalHash = Blake3(All Chunks)
         |
         v
    Hash <= Target?
    /            \
  YES            NO
   |              |
Broadcast     Increment
Proof         Nonce
```

## Technology Stack

| Component | Technology | Version | Purpose |
|-----------|-----------|---------|---------|
| **Language** | Rust | 2021 Edition | System-level performance and safety |
| **Primary Hash** | Blake3 | 1.5.1 | Fast hashing with cryptographic strength |
| **PRNG** | ChaCha20 | via rand_chacha | Deterministic offset generation |
| **CLI Framework** | Clap | 4.4 | Command-line argument parsing |
| **Build Tool** | Cargo | Latest | Package and dependency management |

## Project Structure

```
proof_of_io/
├── src/
│   ├── main.rs          # Entry point, argument parsing, orchestration
│   ├── plot.rs          # Plot file initialization and generation
│   ├── disk.rs          # Disk I/O operations and chunk reading
│   ├── crypto.rs        # Cryptographic hashing operations
│   └── mod declarations
├── Cargo.toml           # Project dependencies and metadata
├── Instructions.md      # Detailed Phase 2 documentation
└── README.md           # This file
```

## Installation

### Requirements

- **OS:** Windows, macOS, or Linux (x86-64)
- **Hardware:** NVMe SSD with PCIe interface
- **Software:** Rust toolchain 1.70+

### Build from Source

1. Clone or navigate to the project directory:
```bash
cd proof_of_io
```

2. Build the project:
```bash
cargo build --release
```

3. Verify the build:
```bash
cargo --version
rustc --version
```

## Usage

### Basic Initialization

Run the default initialization with 50 MB plot:

```bash
cargo run --release
```

### Custom Plot Size

Specify a custom plot size (in bytes):

```bash
cargo run --release -- --plot-size 104857600 --path ./custom.plot
```

### Command-Line Arguments

| Argument | Short | Default | Description |
|----------|-------|---------|-------------|
| `--plot-size` | `-p` | 52428800 (50 MB) | Total bytes for plot file |
| `--path` | `-pa` | ./poio_test.plot | Output path for plot file |

### Example Commands

```bash
# Default 50 MB plot
cargo run --release

# 100 MB plot
cargo run --release -- --plot-size 104857600

# Custom path
cargo run --release -- --path /mnt/ssd/poio.plot

# Combined
cargo run --release -- -p 209715200 -pa ./large_plot.plot
```

## Performance Metrics

Expected performance on consumer-grade NVMe SSD:

| Metric | Value | Notes |
|--------|-------|-------|
| **Random Read Latency** | ~100 µs | Per 4 KB chunk from SSD |
| **Blake3 Computation** | <1 µs | Software hashing overhead |
| **Hash Rate** | 50-80 hashes/sec | Per NVMe drive (I/O limited) |
| **Plot Generation** | ~500 MB/s | Depends on SSD write speed |
| **Plot Size (Default)** | 50 MB | Configurable for testing |

## Module Documentation

### main.rs
Orchestrates the initialization pipeline using loosely-coupled modules. Parses command-line arguments and invokes plot generation.

### plot.rs
Implements deterministic plot file generation using ChaCha20-seeded random data. Supports configurable sizes for testing and production deployment.

### disk.rs
Provides low-level disk I/O operations for reading fixed-size chunks at specified offsets using platform-native file APIs.

### crypto.rs
Wraps Blake3 hashing functionality for seed computation and final hash validation.

## Security Considerations

### Attack Vectors & Mitigations

| Attack Vector | Risk | Mitigation |
|---------------|------|-----------|
| **RAM Drive Attack** | In-memory plot replication | Theoretical analysis proves PCIe bandwidth remains bottleneck |
| **Compression Attack** | Plot file data compression to reduce I/O | Random data generation prevents effective compression |
| **CPU Optimization** | Faster CPU execution | I/O latency dominates computation; CPU gains negligible |
| **ASIC Acceleration** | Custom hardware for disk access | PCIe interface is commodity hardware; no acceleration possible |

## Development & Testing

### Running Tests

```bash
cargo test
```

### Benchmarking

```bash
cargo build --release
time cargo run --release
```

### Debug Build

```bash
cargo build
cargo run
```

## Future Roadmap

### Phase 3 Objectives

- **Network Integration:** Deploy on local Testnet with Rust-native network layer
- **Verification:** Implement lightweight Merkle proof verification for light clients
- **Scaling:** Support multi-terabyte plots (Phase 2 uses 50 MB for testing)
- **Optimization:** Further optimize I/O patterns and memory utilization

### Planned Enhancements

- Distributed mining pool support
- SPV-style light client verification
- Proof compression techniques
- Cross-chain integration

## Dependencies

```toml
[dependencies]
rand_chacha = "0.3.1"    # ChaCha20 PRNG for deterministic offset generation
rand_core = "0.6.4"      # Core traits for random number generation
blake3 = "1.5.1"         # Fast cryptographic hashing
clap = "4.4"             # Command-line argument parsing
```

## Contributing

This is an academic research project. For contributions or inquiries, please contact the project team.

## Team

- Bazil Suhail (Bscs22072)
- Abdullah Masood (Bscs22054)
- Ebad Junaid (Bscs22046)

## References

1. Memory-Hard Functions: When Theory Meets Practice – eScholarship.org
2. Argon2: New Generation of Memory-Hard Functions for Password Hashing and Other Applications
3. Demystifying Crypto-Mining: Analysis and Optimizations of Memory-Hard PoW Algorithms
4. Proof of Space – Chia Network Documentation
5. Green by Design? Investigating the Energy and Carbon Footprint of Chia Network – arXiv
6. Balloon Hashing: A Memory-Hard Function Providing Provable Protection Against Sequential Attacks

## License

Academic Research Project - Phase 2 Submission