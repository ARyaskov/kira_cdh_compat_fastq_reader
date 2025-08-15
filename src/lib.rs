//! Streaming FASTQ reader compatible with CD-HIT input handling.
//!
//! - Plain and `.gz` (auto-detect).
//! - Streaming, record-by-record (no full-file buffering).
//! - CD-HIT-like error policy: skip malformed (default) or return error.
//! - Single-line FASTQ mode by default; multi-line can be enabled via options.
//! - Optional `mmap` for plain files; `zlib` feature for system-zlib parity.
//! - Optional async API behind `async` feature.

pub mod error;
pub mod policy;
pub mod reader;
pub mod record;
mod util;

#[cfg(feature = "async")]
pub mod async_reader;

pub use crate::error::{FastqError, FormatError, IoContext};
pub use crate::policy::{ErrorPolicy, LineMode, ReaderOptions};
pub use crate::reader::{FastqReader, Source};
pub use crate::record::FastqRecord;

#[cfg(feature = "async")]
pub use crate::async_reader::AsyncFastqReader;
