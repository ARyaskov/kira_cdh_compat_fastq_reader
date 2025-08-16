use crate::error::{FastqError, FormatError, IoContext};
use crate::policy::{ErrorPolicy, LineMode, ReaderOptions};
use crate::record::FastqRecord;
use crate::util::{looks_like_gzip, open_file};

#[cfg(feature = "gzip")]
use flate2::read::MultiGzDecoder;
#[cfg(feature = "mmap")]
use memmap2::Mmap;

use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum Source {
    Path(PathBuf),
    Reader,
}

/// Sync FASTQ reader (plain/.gz), streaming.
pub struct FastqReader {
    src: Source,
    rdr: Box<dyn BufRead + Send>,
    opts: ReaderOptions,
    line_num: u64,
    byte_pos: u64,
    pending_header: Option<String>,
}

impl FastqReader {
    /// Open from a file path. Auto-detect `.gz` by extension or magic bytes.
    pub fn from_path<P: AsRef<Path>>(path: P, opts: ReaderOptions) -> Result<Self, FastqError> {
        let path = path.as_ref();
        let f = open_file(path).map_err(|e| {
            FastqError::io_err(
                e,
                IoContext {
                    byte_pos: 0,
                    line_num: 0,
                },
            )
        })?;

        let is_gz = path.extension().and_then(|s| s.to_str()) == Some("gz")
            || looks_like_gzip(&f).unwrap_or(false);

        let rdr: Box<dyn BufRead + Send> = if is_gz {
            #[cfg(feature = "gzip")]
            {
                let dec = MultiGzDecoder::new(f);
                Box::new(BufReader::with_capacity(256 * 1024, dec))
            }
            #[cfg(not(feature = "gzip"))]
            {
                return Err(FastqError::fmt_err(
                    FormatError::MissingHeader,
                    IoContext {
                        byte_pos: 0,
                        line_num: 0,
                    },
                ));
            }
        } else {
            #[cfg(feature = "mmap")]
            {
                let mmap = unsafe { Mmap::map(&f) }.map_err(|e| {
                    FastqError::io_err(
                        e,
                        IoContext {
                            byte_pos: 0,
                            line_num: 0,
                        },
                    )
                })?;
                let cursor = std::io::Cursor::new(mmap);
                Box::new(BufReader::with_capacity(512 * 1024, cursor))
            }
            #[cfg(not(feature = "mmap"))]
            {
                Box::new(BufReader::with_capacity(256 * 1024, f))
            }
        };

        Ok(Self {
            src: Source::Path(path.to_path_buf()),
            rdr,
            opts,
            line_num: 0,
            byte_pos: 0,
            pending_header: None,
        })
    }

    /// Wrap an arbitrary `BufRead` (stdin, etc.).
    pub fn from_bufread<R>(reader: R, opts: ReaderOptions) -> Self
    where
        R: BufRead + Send + 'static,
    {
        Self {
            src: Source::Reader,
            rdr: Box::new(reader),
            opts,
            line_num: 0,
            byte_pos: 0,
            pending_header: None,
        }
    }

    pub fn next(&mut self) -> Option<Result<FastqRecord, FastqError>> {
        loop {
            match self.read_one() {
                Ok(Some(rec)) => return Some(Ok(rec)),
                Ok(None) => return None,
                Err(err) => {
                    if self.opts.error_policy == ErrorPolicy::Skip {
                        log::warn!("skipping malformed record: {err}");
                        if !self.resync_to_next_header() {
                            return None;
                        }
                        continue;
                    } else {
                        return Some(Err(err));
                    }
                }
            }
        }
    }

    fn read_line(&mut self, buf: &mut String) -> io::Result<usize> {
        buf.clear();
        let n = self.rdr.read_line(buf)?;
        if n > 0 {
            self.line_num += 1;
            self.byte_pos += n as u64;
            if buf.ends_with('\n') {
                buf.pop();
            }
            if buf.ends_with('\r') {
                buf.pop();
            }
        }
        Ok(n)
    }

    fn read_one(&mut self) -> Result<Option<FastqRecord>, FastqError> {
        let header = if let Some(h) = self.pending_header.take() {
            h
        } else {
            let mut h = String::with_capacity(128);
            loop {
                let n = self
                    .read_line(&mut h)
                    .map_err(|e| FastqError::io_err(e, self.ctx()))?;
                if n == 0 {
                    return Ok(None);
                }
                if !h.is_empty() {
                    if h.starts_with('\u{FEFF}') {
                        h.drain(..'\u{FEFF}'.len_utf8());
                        if h.is_empty() {
                            continue;
                        }
                    }
                    break;
                }
            }
            h
        };

        if !header.starts_with('@') {
            if self.opts.fastq_only && header.starts_with('>') {
                return Err(FastqError::fmt_err(
                    FormatError::FastaHeaderDetected,
                    self.ctx(),
                ));
            }
            let ch = header.chars().next().unwrap_or('\0');
            let bytes = header.as_bytes();
            let hex = bytes[..bytes.len().min(4)]
                .iter()
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join(" ");
            let msg = format!(
                "expected header '@' at start of record, got {:?} (U+{:04X}); first bytes: {}",
                ch, ch as u32, hex
            );
            return Err(FastqError::io_err(
                io::Error::new(io::ErrorKind::InvalidData, msg),
                self.ctx(),
            ));
        }

        let mut parts = header[1..].splitn(2, char::is_whitespace);
        let id = parts.next().unwrap_or("").to_string();
        let desc = parts.next().map(|s| s.trim().to_string());

        let mut line = String::with_capacity(256);

        match self.opts.line_mode {
            LineMode::Single => {
                let n = self
                    .read_line(&mut line)
                    .map_err(|e| FastqError::io_err(e, self.ctx()))?;
                if n == 0 || line.is_empty() {
                    return Err(FastqError::fmt_err(FormatError::EmptySequence, self.ctx()));
                }
                let seq = line.as_bytes().to_vec();

                let n = self
                    .read_line(&mut line)
                    .map_err(|e| FastqError::io_err(e, self.ctx()))?;
                if n == 0 || !line.starts_with('+') {
                    return Err(FastqError::fmt_err(FormatError::MissingPlus, self.ctx()));
                }

                let n = self
                    .read_line(&mut line)
                    .map_err(|e| FastqError::io_err(e, self.ctx()))?;
                if n == 0 {
                    return Err(FastqError::fmt_err(FormatError::UnexpectedEof, self.ctx()));
                }
                let qual = line.as_bytes().to_vec();

                if qual.len() != seq.len() {
                    return Err(FastqError::fmt_err(
                        FormatError::LengthMismatch {
                            seq: seq.len(),
                            qual: qual.len(),
                        },
                        self.ctx(),
                    ));
                }
                Ok(Some(FastqRecord {
                    id,
                    desc,
                    seq,
                    qual,
                }))
            }
            LineMode::Multi => {
                let mut seq = Vec::with_capacity(256);
                loop {
                    let n = self
                        .read_line(&mut line)
                        .map_err(|e| FastqError::io_err(e, self.ctx()))?;
                    if n == 0 {
                        return Err(FastqError::fmt_err(FormatError::UnexpectedEof, self.ctx()));
                    }
                    if line.starts_with('+') {
                        break;
                    }
                    seq.extend_from_slice(line.as_bytes());
                }
                if seq.is_empty() {
                    return Err(FastqError::fmt_err(FormatError::EmptySequence, self.ctx()));
                }

                let mut qual = Vec::with_capacity(seq.len());
                while qual.len() < seq.len() {
                    let n = self
                        .read_line(&mut line)
                        .map_err(|e| FastqError::io_err(e, self.ctx()))?;
                    if n == 0 {
                        return Err(FastqError::fmt_err(FormatError::UnexpectedEof, self.ctx()));
                    }
                    qual.extend_from_slice(line.as_bytes());
                }

                if qual.len() != seq.len() {
                    return Err(FastqError::fmt_err(
                        FormatError::LengthMismatch {
                            seq: seq.len(),
                            qual: qual.len(),
                        },
                        self.ctx(),
                    ));
                }
                Ok(Some(FastqRecord {
                    id,
                    desc,
                    seq,
                    qual,
                }))
            }
        }
    }

    fn resync_to_next_header(&mut self) -> bool {
        let mut buf = String::with_capacity(256);
        loop {
            match self.read_line(&mut buf) {
                Ok(0) => return false,
                Ok(_) if buf.starts_with('@') => {
                    self.pending_header = Some(buf.clone());
                    return true;
                }
                Ok(_) => {}
                Err(_) => return false,
            }
        }
    }

    #[inline]
    fn ctx(&self) -> IoContext {
        IoContext {
            byte_pos: self.byte_pos,
            line_num: self.line_num,
        }
    }
}

impl Iterator for FastqReader {
    type Item = Result<FastqRecord, FastqError>;
    fn next(&mut self) -> Option<Self::Item> {
        FastqReader::next(self)
    }
}
