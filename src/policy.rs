/// Error handling policy compatible with CD-HIT flows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorPolicy {
    /// Skip malformed records and continue (CD-HIT-like behavior).
    Skip,
    /// Return the first error to the caller (strict).
    Return,
}

/// How sequence/quality lines are laid out in FASTQ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineMode {
    /// Sequence and quality occupy exactly one line each (CD-HIT typical).
    Single,
    /// Sequence/quality may span multiple lines (general FASTQ).
    Multi,
}

#[derive(Debug, Clone)]
pub struct ReaderOptions {
    pub error_policy: ErrorPolicy,
    pub fastq_only: bool,
    pub line_mode: LineMode,
}

impl Default for ReaderOptions {
    fn default() -> Self {
        Self {
            error_policy: ErrorPolicy::Skip,
            fastq_only: true,
            line_mode: LineMode::Single, // default to single-line for CD-HIT compatibility
        }
    }
}
