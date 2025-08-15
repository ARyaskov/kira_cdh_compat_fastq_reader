#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastqRecord {
    pub id: String,
    pub desc: Option<String>,
    pub seq: Vec<u8>,
    pub qual: Vec<u8>,
}

impl FastqRecord {
    #[inline]
    pub fn len(&self) -> usize {
        self.seq.len()
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.seq.is_empty()
    }
}
