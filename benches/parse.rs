use criterion::{Criterion, criterion_group, criterion_main};
use kira_cdh_compat_fastq_reader::{ErrorPolicy, FastqReader, LineMode, ReaderOptions};
use std::io::BufReader;

fn bench_parse(c: &mut Criterion) {
    let mut data = String::new();
    for i in 0..2000 {
        data.push_str(&format!("@r{i}\nACGTACGTACGTACGT\n+\n################\n"));
    }
    c.bench_function("parse_2000_singleline", |b| {
        b.iter(|| {
            let rdr = BufReader::new(data.as_bytes());
            let fq = FastqReader::from_bufread(
                rdr,
                ReaderOptions {
                    error_policy: ErrorPolicy::Return,
                    fastq_only: true,
                    line_mode: LineMode::Single,
                },
            );
            let mut n = 0usize;
            for rec in fq {
                let r = rec.unwrap();
                n += r.len();
            }
            n
        })
    });
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
