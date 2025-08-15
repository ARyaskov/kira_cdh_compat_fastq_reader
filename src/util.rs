use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

pub fn looks_like_gzip<R: Read + Seek>(mut r: R) -> io::Result<bool> {
    let mut magic = [0u8; 2];
    let pos = r.seek(SeekFrom::Current(0))?;
    let n = r.read(&mut magic)?;
    r.seek(SeekFrom::Start(pos))?;
    Ok(n >= 2 && magic == [0x1F, 0x8B])
}

pub fn open_file(path: &std::path::Path) -> io::Result<File> {
    std::fs::File::open(path)
}
