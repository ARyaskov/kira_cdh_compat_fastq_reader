use std::io;
use thiserror::Error;

#[derive(Debug, Clone, Copy)]
pub struct IoContext {
    pub byte_pos: u64,
    pub line_num: u64,
}

#[derive(Debug, Error)]
pub enum FormatError {
    #[error("expected header '@' at start of record")]
    MissingHeader,
    #[error("found FASTA header '>' where FASTQ '@' expected")]
    FastaHeaderDetected,
    #[error("missing '+' separator line")]
    MissingPlus,
    #[error("unexpected EOF inside record")]
    UnexpectedEof,
    #[error("quality length ({qual}) does not match sequence length ({seq})")]
    LengthMismatch { seq: usize, qual: usize },
    #[error("empty sequence")]
    EmptySequence,
}

#[derive(Debug, Error)]
pub enum FastqError {
    #[error("I/O error at {ctx:?}: {source}")]
    Io {
        #[source]
        source: io::Error,
        ctx: IoContext,
    },
    #[error("format error at {ctx:?}: {source}")]
    Format {
        #[source]
        source: FormatError,
        ctx: IoContext,
    },
}

impl FastqError {
    pub(crate) fn io_err(source: io::Error, ctx: IoContext) -> Self {
        Self::Io { source, ctx }
    }
    pub(crate) fn fmt_err(source: FormatError, ctx: IoContext) -> Self {
        Self::Format { source, ctx }
    }
}
