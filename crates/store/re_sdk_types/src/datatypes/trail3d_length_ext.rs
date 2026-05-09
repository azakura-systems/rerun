use super::Trail3DLength;

impl Trail3DLength {
    /// Creates a trail length for both temporal and sequence timelines.
    #[inline]
    pub fn new(seconds: f64, ticks: u64) -> Self {
        Self { seconds, ticks }
    }
}

impl From<(f64, u64)> for Trail3DLength {
    #[inline]
    fn from((seconds, ticks): (f64, u64)) -> Self {
        Self::new(seconds, ticks)
    }
}
