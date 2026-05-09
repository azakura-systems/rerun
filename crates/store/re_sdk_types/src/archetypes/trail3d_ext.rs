use super::Trail3D;

impl Trail3D {
    /// Creates a Trail3D from one point.
    pub fn from_point(point: impl Into<crate::components::Trail3DPoint>) -> Self {
        Self::new(point)
    }
}
