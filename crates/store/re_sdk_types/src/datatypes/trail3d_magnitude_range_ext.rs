use super::Trail3DMagnitudeRange;

const AUTO_MODE: u8 = 1;
const FIXED_MODE: u8 = 2;
const DEFAULT_MIN: f64 = 0.0;
const DEFAULT_MAX: f64 = 10.0;

impl Trail3DMagnitudeRange {
    /// Creates an automatically derived magnitude range.
    pub fn auto() -> Self {
        Self {
            mode: AUTO_MODE,
            min: DEFAULT_MIN,
            max: DEFAULT_MAX,
        }
    }

    /// Creates a fixed magnitude range.
    pub fn fixed(range: impl Into<[f64; 2]>) -> Self {
        let [min, max] = range.into();
        Self {
            mode: FIXED_MODE,
            min,
            max,
        }
    }

    /// Returns true when the visible trail magnitudes should determine the range.
    pub fn is_auto(&self) -> bool {
        self.mode != FIXED_MODE
    }

    /// Returns true when the stored fixed range should be used.
    pub fn is_fixed(&self) -> bool {
        self.mode == FIXED_MODE
    }

    /// Returns the stored fixed range.
    pub fn fixed_range(&self) -> [f64; 2] {
        [self.min, self.max]
    }
}

impl From<[f64; 2]> for Trail3DMagnitudeRange {
    fn from(range: [f64; 2]) -> Self {
        Self::fixed(range)
    }
}

impl From<(f64, f64)> for Trail3DMagnitudeRange {
    fn from(range: (f64, f64)) -> Self {
        Self::fixed([range.0, range.1])
    }
}
