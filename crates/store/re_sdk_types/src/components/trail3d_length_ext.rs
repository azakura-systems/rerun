use super::Trail3DLength;

impl Trail3DLength {
    /// Creates a trail length for both temporal and sequence timelines.
    #[inline]
    pub fn new(seconds: f64, ticks: u64) -> Self {
        Self(crate::datatypes::Trail3DLength::new(seconds, ticks))
    }
}
