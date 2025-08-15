# Kira Bio Tools CD-Hit-compatible FASTQ reader

Streaming FASTQ reader with **CD-HIT–compatible** input handling (plain and `.gz`), a **safe, idiomatic Rust API**, and optional **async** support.

- **Single-line FASTQ by default** (sequence and quality occupy exactly one line each) to match common CD-HIT expectations.
- **Multi-line FASTQ** supported via an option for broader compatibility.
- **Gzip auto-detection** by extension or magic bytes.
- **Error policy**: skip malformed records (CD-HIT-like, default) or fail fast.
- **Streaming**: process records one-by-one without loading the whole file.
- **Optional `mmap`** for faster plain-file reads.
- **Async API** behind the `async` feature (Tokio + async-compression).
- **MSRV** pinned to **Rust 1.85**; `edition = 2024`.

---

## Table of contents

- [Status](#status)
- [Installation](#installation)
- [Features](#features)
- [Design goals](#design-goals)
- [Quick start (sync)](#quick-start-sync)
- [Quick start (async)](#quick-start-async)
- [Single-line vs multi-line FASTQ](#single-line-vs-multi-line-fastq)
- [Error policy](#error-policy)
- [Resynchronization behavior](#resynchronization-behavior)
- [API overview](#api-overview)
- [Performance notes](#performance-notes)
- [Testing & benches](#testing--benches)
- [Versioning & MSRV](#versioning--msrv)
- [License](#license)

---

## Status

- Production-ready streaming reader for FASTQ (plain and gzip).
- Cross-platform CI for Linux, macOS, Windows.
- Intended for use in pipelines where CD-HIT input behavior must be mirrored.

---

## Installation

```toml
[dependencies]
kira_cdh_compat_fastq_reader = "*"
````

Optional features:

```toml
[dependencies]
kira_cdh_compat_fastq_reader = { version = "0.1", features = ["async", "mmap", "zlib"] }
```

* `gzip` — enabled by default (gzip via `flate2` with miniz\_oxide backend).
* `zlib` — switch `flate2` to system zlib backend (closer to CD-HIT’s zlib path).
* `mmap` — enable `memmap2` for plain files (reduces syscalls).
* `async` — enable async API (Tokio + async-compression).

**MSRV:** 1.85.0 or newer (pinned).

---

## Features

* **CD-HIT–compatible defaults:** single-line mode and a resilient “skip-bad-and-continue” policy.
* **Auto gzip detection:** by `.gz` extension or magic bytes (`1F 8B`).
* **Streaming iterator:** reads record-by-record; constant memory overhead regardless of file size.
* **Clear error reporting:** format errors include line/byte context.
* **Minimal dependencies:** core functionality keeps dependency surface small; performance extras are opt-in.

---

## Design goals

* **Safety first:** no unsafe parsing; owned record buffers; clear error types.
* **Predictable behavior:** strong defaults that mirror CD-HIT expectations.
* **Composability:** easy to integrate in larger pipelines (sync or async).
* **KISS/DRY:** keep the public API small and focused.

---

## Quick start (sync)

```rust
use kira_cdh_compat_fastq_reader::{FastqReader, ReaderOptions, ErrorPolicy, LineMode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // CD-HIT–compatible defaults:
    let opts = ReaderOptions {
        error_policy: ErrorPolicy::Skip, // keep going on malformed records
        fastq_only: true,                // reject FASTA '>' headers
        line_mode: LineMode::Single,     // single-line seq/qual
    };

    let mut rdr = FastqReader::from_path("reads.fastq.gz", opts)?;

    for rec in &mut rdr {
        let rec = match rec {
            Ok(r) => r,
            Err(e) => { eprintln!("skipped: {e}"); continue; }
        };
        println!("id={} len={}", rec.id, rec.len());
    }
    Ok(())
}
```

**From stdin**:

```rust
use std::io::{self, BufReader};
use kira_cdh_compat_fastq_reader::{FastqReader, ReaderOptions, ErrorPolicy, LineMode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opts = ReaderOptions { error_policy: ErrorPolicy::Return, fastq_only: true, line_mode: LineMode::Single };
    let stdin = io::stdin();
    let rdr = BufReader::new(stdin.lock());
    let mut fq = FastqReader::from_bufread(rdr, opts);
    for rec in &mut fq {
        let r = rec?;
        println!("{}", r.id);
    }
    Ok(())
}
```

---

## Quick start (async)

> Enable the `async` feature:
> `kira_cdh_compat_fastq_reader = { version = "0.1", features = ["async"] }`

```rust
use kira_cdh_compat_fastq_reader::{AsyncFastqReader, ReaderOptions, ErrorPolicy, LineMode};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opts = ReaderOptions {
        error_policy: ErrorPolicy::Skip,
        fastq_only: true,
        line_mode: LineMode::Single,
    };

    let mut rdr = AsyncFastqReader::from_path("reads.fastq.gz", opts).await?;

    while let Some(item) = rdr.next_record().await {
        let rec = match item {
            Ok(r) => r,
            Err(e) => { eprintln!("skipped: {e}"); continue; }
        };
        println!("id={} len={}", rec.id, rec.len());
    }
    Ok(())
}
```

You can also wrap any `AsyncBufRead` via:

```rust
// AsyncFastqReader::from_async_bufread(reader, opts)
```

---

## Single-line vs multi-line FASTQ

* **Single-line (default):** after the `@header`, **sequence** is exactly one line, `+` is one line, **quality** is exactly one line. This matches typical Illumina output and how CD-HIT often sees inputs.
* **Multi-line:** sequence and/or quality may span multiple lines. Enable via:

  ```rust
  ReaderOptions { line_mode: LineMode::Multi, ..Default::default() }
  ```

**Note:** Single-line mode is both stricter and faster. If your datasets are multi-line, switch to `LineMode::Multi`.

---

## Error policy

```rust
enum ErrorPolicy {
    Skip,   // default: skip malformed records and continue (CD-HIT-like)
    Return, // fail fast on first malformed record
}
```

Typical format errors include:

* Missing header `@` (or encountering FASTA `>` in FASTQ-only mode).
* Missing `+` line.
* Unexpected EOF inside a record.
* Length mismatch between sequence and quality.
* Empty sequence.

All errors carry an **I/O context** (byte offset and line number).

---

## Resynchronization behavior

With `ErrorPolicy::Skip`, the parser attempts to **resynchronize** at the next line starting with `@`. This mirrors the robust “keep going” behavior often expected in CD-HIT pipelines when inputs contain occasional malformed records.

---

## API overview

**Types**

* `FastqReader` — synchronous streaming reader (plain or `.gz`).
* `AsyncFastqReader` — asynchronous streaming reader (feature `async`).
* `FastqRecord` — `{ id, desc: Option<String>, seq: Vec<u8>, qual: Vec<u8> }`.
* `ReaderOptions` — `{ error_policy, fastq_only, line_mode }`.
* `ErrorPolicy` — `Skip` or `Return`.
* `LineMode` — `Single` or `Multi`.
* `FastqError` / `FormatError` — detailed error types with context.

**Construction**

```rust
// sync
let mut r = FastqReader::from_path("reads.fastq.gz", opts)?;
// or
let mut r = FastqReader::from_bufread(my_buf_reader, opts);

// async
let mut ar = AsyncFastqReader::from_path("reads.fastq.gz", opts).await?;
// or
let mut ar = AsyncFastqReader::from_async_bufread(my_async_bufread, opts);
```

**Iteration**

```rust
// sync
for item in &mut r {
    let rec = item?; // or handle Skip policy
    // ...
}

// async
while let Some(item) = ar.next_record().await {
    let rec = item?; // or handle Skip policy
    // ...
}
```

---

## Performance notes

* **Plain FASTQ + `mmap`** (`--features mmap`): can reduce syscalls and improve throughput on fast storage (commonly +5–30% vs buffered reads).
* **Gzip**:

    * Default `flate2` backend (miniz\_oxide) provides solid performance.
    * `--features zlib` switches to system zlib for closer parity with CD-HIT’s zlib path.
* **I/O-bound** workloads benefit most from larger buffers and sequential access patterns; CPU-bound cases (e.g., heavy downstream processing) usually dwarf parse costs.

Use `cargo bench` to evaluate on your hardware and datasets.

---

## Testing & benches

```bash
# default features (gzip enabled)
cargo test

# all features
cargo test --all-features

# benches
cargo bench
```

Tests cover:

* Basic parsing (single-line).
* Gzip files.
* Skip vs Return policies.
* Async parsing (behind `async` feature).

---

## Versioning & MSRV

* **MSRV:** **>=1.85**. 
* **SemVer:** public API follows semantic versioning. Breaking changes trigger a major version bump.

---

## License

Licensed under **GPLv2** like a CD-Hit.

