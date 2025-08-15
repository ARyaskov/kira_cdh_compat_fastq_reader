use kira_cdh_compat_fastq_reader::{ErrorPolicy, LineMode, ReaderOptions};
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

#[cfg(feature = "gzip")]
#[test]
fn parse_gz_file_single_line() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("sample.fastq.gz");
    {
        let f = File::create(&path).unwrap();
        let mut enc = flate2::write::GzEncoder::new(f, flate2::Compression::fast());
        writeln!(enc, "@x").unwrap();
        writeln!(enc, "ACGT").unwrap();
        writeln!(enc, "+").unwrap();
        writeln!(enc, "!!!!").unwrap();
        enc.finish().unwrap();
    }

    let mut fq = kira_cdh_compat_fastq_reader::FastqReader::from_path(
        &path,
        ReaderOptions {
            error_policy: ErrorPolicy::Return,
            fastq_only: true,
            line_mode: LineMode::Single,
        },
    )
    .expect("open gz");

    let rec = fq.next().unwrap().unwrap();
    assert_eq!(rec.id, "x");
    assert_eq!(rec.seq, b"ACGT");
    assert_eq!(rec.qual, b"!!!!");
    assert!(fq.next().is_none());
}
