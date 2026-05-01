# Proof of I/O (PoIO) Implementation Overview

This code implements a prototype for a **Proof of Space** or **Proof of Capacity** consensus mechanism. Instead of using raw CPU power to solve arbitrary math problems (like Bitcoin's Proof of Work), this system requires users to "plot" (reserve) disk space and then prove they are still storing that data via random reads.



## Core Components & Workflow

### 1. The Plotting Phase (`plot.rs`)
Before mining can begin, the node must initialize a "plot."
* **Action:** It creates a large file (defaulting to 50MB in this code) and fills it with pseudo-random data.
* **The Logic:** It uses `ChaCha8Rng` seeded with a `genesis_seed`. This ensures the data is deterministic but appears random. 
* **Purpose:** In a production system, this binds the storage to a specific identity or public key, ensuring you can't "borrow" someone else's plot.

### 2. The Mining Phase (`main.rs`)
Once the plot exists, the node enters a mining loop for every new block:
* **Deterministic Randomness:** It combines a block header and a `nonce` (counter) to create a unique seed.
* **Random I/O Selection:** Using that seed, it selects **128 random locations** (offsets) within the plot file.
* **The "I/O" Operation:** The code "jumps" around the disk using `seek` to read 4KB chunks from those 128 specific locations.
* **Hashing:** It aggregates all 128 chunks and hashes them together using **BLAKE3** to produce a `final_hash`.



### 3. Verification & Difficulty (`check_difficulty`)
A block is only considered "found" if the `final_hash` meets a specific target.
* **Difficulty:** This parameter defines how many leading zero bits the hash must have.
* **Iteration:** If the hash fails the check, the code increments the `nonce`, picks 128 *new* random offsets, and repeats the process.

---

## Why is this "Proof of I/O"?

This design shifts the economic and hardware requirements of mining away from traditional computation:

* **Space > Electricity:** To increase winning odds, you need a larger plot file (more "lottery tickets") rather than a faster processor.
* **I/O Latency Bottleneck:** Because each attempt requires 128 random reads, the bottleneck is the **IOPS (Input/Output Operations Per Second)** of the storage media.
* **Anti-ASIC:** It is difficult to build specialized chips to bypass this, as the chip would still be limited by the physical seek time and read speed of the hard drive or SSD.

---

## Modular Structure

| Module | Responsibility |
| :--- | :--- |
| **`crypto`** | Handles the **BLAKE3** hashing operations. |
| **`disk`** | Manages low-level file `seek` and `read_exact` operations. |
| **`plot`** | Handles the initial generation of the deterministic data file. |
| **`main`** | Orchestrates the mining loop and parses CLI arguments via `clap`. |

> **Summary:** This is a simplified version of the logic used by blockchains like **Chia** or **Spacemesh**, effectively trading hard drive capacity and throughput for network security.