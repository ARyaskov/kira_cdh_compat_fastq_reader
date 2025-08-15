use kira_cdh_compat_fastq_reader::{ErrorPolicy, FastqReader, LineMode, ReaderOptions};
use std::io::BufReader;

const SAMPLE: &str = "\
@read1 desc
ACGTN
+
!!!!!
@read2
ACGT
+
####";

#[test]
fn parse_two_records_single_line() {
    let rdr = BufReader::new(SAMPLE.as_bytes());
    let mut fq = FastqReader::from_bufread(
        rdr,
        ReaderOptions {
            error_policy: ErrorPolicy::Return,
            fastq_only: true,
            line_mode: LineMode::Single,
        },
    );

    let r1 = fq.next().unwrap().unwrap();
    assert_eq!(r1.id, "read1");
    assert_eq!(r1.desc.as_deref(), Some("desc"));
    assert_eq!(r1.seq, b"ACGTN");
    assert_eq!(r1.qual, b"!!!!!");

    let r2 = fq.next().unwrap().unwrap();
    assert_eq!(r2.id, "read2");
    assert_eq!(r2.desc, None);
    assert_eq!(r2.seq, b"ACGT");
    assert_eq!(r2.qual, b"####");

    assert!(fq.next().is_none());
}

#[test]
fn multi_line_rejected_in_single_mode() {
    let bad = "\
@r1
ACG
T
+
####
";
    let rdr = BufReader::new(bad.as_bytes());
    let mut fq = FastqReader::from_bufread(
        rdr,
        ReaderOptions {
            error_policy: ErrorPolicy::Return,
            fastq_only: true,
            line_mode: LineMode::Single,
        },
    );
    // second seq line will cause MissingPlus error
    let err = fq.next().unwrap().unwrap_err();
    match err {
        kira_cdh_compat_fastq_reader::FastqError::Format { .. } => {}
        _ => panic!("expected format error"),
    }
}

#[test]
fn length_mismatch_skipped_in_skip_mode() {
    let bad = "\
@r1
ACGT
+
###
@r2
A
+
#";
    let rdr = BufReader::new(bad.as_bytes());
    let mut fq = FastqReader::from_bufread(
        rdr,
        ReaderOptions {
            error_policy: ErrorPolicy::Skip,
            fastq_only: true,
            line_mode: LineMode::Single,
        },
    );

    // r1 malformed -> resync to @r2
    let r = fq.next().unwrap().unwrap();
    assert_eq!(r.id, "r2");
    assert_eq!(r.seq, b"A");
    assert_eq!(r.qual, b"#");
    assert!(fq.next().is_none());
}
