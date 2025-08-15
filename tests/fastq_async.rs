#[cfg(feature = "async")]
mod t {
    use kira_cdh_compat_fastq_reader::{AsyncFastqReader, ErrorPolicy, LineMode, ReaderOptions};
    use tempfile::tempdir;
    use tokio::fs::File;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn async_parse_plain() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.fastq");
        {
            let mut f = File::create(&path).await.unwrap();
            f.write_all(b"@id\nACGT\n+\n!!!!\n").await.unwrap();
        }
        let mut fq = AsyncFastqReader::from_path(
            &path,
            ReaderOptions {
                error_policy: ErrorPolicy::Return,
                fastq_only: true,
                line_mode: LineMode::Single,
            },
        )
        .await
        .unwrap();

        if let Some(Ok(rec)) = fq.next_record().await {
            assert_eq!(rec.id, "id");
            assert_eq!(rec.seq, b"ACGT");
            assert_eq!(rec.qual, b"!!!!");
        } else {
            panic!("no record");
        }
        assert!(fq.next_record().await.is_none());
    }
}
