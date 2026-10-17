#[derive(Clone)]
pub struct MediaTrack {
    pub id: i64,
    pub label: String,
    pub selected: bool,
}

#[derive(Clone, Default)]
pub struct CacheCoverage {
    pub total_bytes: u64,
    pub ranges: Vec<(u64, u64)>,
}
