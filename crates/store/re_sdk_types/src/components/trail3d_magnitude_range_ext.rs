use super::Trail3DMagnitudeRange;

impl Trail3DMagnitudeRange {
    /// Creates an automatically derived magnitude range.
    pub fn auto() -> Self {
        Self(crate::datatypes::Trail3DMagnitudeRange::auto())
    }

    /// Creates a fixed magnitude range.
    pub fn fixed(range: impl Into<[f64; 2]>) -> Self {
        Self(crate::datatypes::Trail3DMagnitudeRange::fixed(range))
    }
}
