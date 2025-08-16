#![cfg(feature = "async")]

use crate::error::{FastqError, FormatError, IoContext};
use crate::policy::{ErrorPolicy, LineMode, ReaderOptions};
use crate::record::FastqRecord;

use async_compression::tokio::bufread::GzipDecoder;
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::{self, AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};

#[derive(Debug)]
pub enum AsyncSource {
    Path(PathBuf),
    Reader,
}

/// Async FASTQ reader (plain/.gz), streaming.
pub struct AsyncFastqReader {
    src: AsyncSource,
    rdr: BufReader<Box<dyn AsyncBufRead + Unpin + Send>>,
    opts: ReaderOptions,
    line_num: u64,
    byte_pos: u64,
    pending_header: Option<String>,
}

impl AsyncFastqReader {
    /// Open async from path; `.gz` auto-detect by extension or magic bytes.
    pub async fn from_path<P: AsRef<Path>>(
        path: P,
        opts: ReaderOptions,
    ) -> Result<Self, FastqError> {
        let path = path.as_ref().to_path_buf();
        let mut f = File::open(&path).await.map_err(|e| {
            FastqError::io_err(
                e,
                IoContext {
                    byte_pos: 0,
                    line_num: 0,
                },
            )
        })?;

        let is_gz = path.extension().and_then(|s| s.to_str()) == Some("gz")
            || looks_like_gzip_async(&mut f).await.unwrap_or(false);

        let inner: Box<dyn AsyncBufRead + Unpin + Send> = if is_gz {
            let gz = GzipDecoder::new(BufReader::with_capacity(256 * 1024, f));
            Box::new(BufReader::with_capacity(256 * 1024, gz))
        } else {
            Box::new(BufReader::with_capacity(256 * 1024, f))
        };

        let rdr = BufReader::with_capacity(256 * 1024, inner);

        Ok(Self {
            src: AsyncSource::Path(path),
            rdr,
            opts,
            line_num: 0,
            byte_pos: 0,
            pending_header: None,
        })
    }

    /// Wrap any async `AsyncBufRead`.
    pub fn from_async_bufread<R>(reader: R, opts: ReaderOptions) -> Self
    where
        R: AsyncBufRead + Unpin + Send + 'static,
    {
        let inner: Box<dyn AsyncBufRead + Unpin + Send> =
            Box::new(BufReader::with_capacity(256 * 1024, reader));
        let rdr = BufReader::with_capacity(256 * 1024, inner);
        Self {
            src: AsyncSource::Reader,
            rdr,
            opts,
            line_num: 0,
            byte_pos: 0,
            pending_header: None,
        }
    }

    /// Fetch next record (async).
    pub async fn next_record(&mut self) -> Option<Result<FastqRecord, FastqError>> {
        loop {
            match self.read_one().await {
                Ok(Some(rec)) => return Some(Ok(rec)),
                Ok(None) => return None,
                Err(err) => {
                    if self.opts.error_policy == ErrorPolicy::Skip {
                        log::warn!("skipping malformed record: {err}");
                        if !self.resync_to_next_header().await {
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

    async fn read_line(&mut self, buf: &mut String) -> io::Result<usize> {
        buf.clear();
        let n = self.rdr.read_line(buf).await?;
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

    async fn read_one(&mut self) -> Result<Option<FastqRecord>, FastqError> {
        // header
        let mut header = if let Some(h) = self.pending_header.take() {
            h
        } else {
            let mut h = String::with_capacity(128);
            loop {
                let n = self
                    .read_line(&mut h)
                    .await
                    .map_err(|e| FastqError::io_err(e, self.ctx()))?;
                if n == 0 {
                    return Ok(None);
                }
                if !h.is_empty() {
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
            return Err(FastqError::fmt_err(FormatError::MissingHeader, self.ctx()));
        }

        let mut parts = header[1..].splitn(2, char::is_whitespace);
        let id = parts.next().unwrap_or("").to_string();
        let desc = parts.next().map(|s| s.trim().to_string());

        let mut line = String::with_capacity(256);

        match self.opts.line_mode {
            LineMode::Single => {
                // seq
                let n = self
                    .read_line(&mut line)
                    .await
                    .map_err(|e| FastqError::io_err(e, self.ctx()))?;
                if n == 0 {
                    return Err(FastqError::fmt_err(FormatError::UnexpectedEof, self.ctx()));
                }
                if line.is_empty() {
                    return Err(FastqError::fmt_err(FormatError::EmptySequence, self.ctx()));
                }
                let seq = line.as_bytes().to_vec();

                // plus
                let n = self
                    .read_line(&mut line)
                    .await
                    .map_err(|e| FastqError::io_err(e, self.ctx()))?;
                if n == 0 {
                    return Err(FastqError::fmt_err(FormatError::UnexpectedEof, self.ctx()));
                }
                if !line.starts_with('+') {
                    return Err(FastqError::fmt_err(FormatError::MissingPlus, self.ctx()));
                }

                // qual
                let n = self
                    .read_line(&mut line)
                    .await
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
                let mut seq = Vec::<u8>::with_capacity(256);
                loop {
                    let n = self
                        .read_line(&mut line)
                        .await
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

                let mut qual = Vec::<u8>::with_capacity(seq.len());
                while qual.len() < seq.len() {
                    let n = self
                        .read_line(&mut line)
                        .await
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

    async fn resync_to_next_header(&mut self) -> bool {
        let mut buf = String::with_capacity(256);
        loop {
            match self.read_line(&mut buf).await {
                Ok(0) => return false,
                Ok(_) => {
                    if buf.starts_with('@') {
                        self.pending_header = Some(buf.clone());
                        return true;
                    }
                }
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

async fn looks_like_gzip_async(f: &mut File) -> io::Result<bool> {
    let pos = f.stream_position().await?;
    let mut magic = [0u8; 2];
    let n = f.read(&mut magic).await?;
    f.seek(SeekFrom::Start(pos)).await?;
    Ok(n >= 2 && magic == [0x1F, 0x8B])
}
