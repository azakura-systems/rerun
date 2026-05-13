use super::Trail3DPoint;

impl Trail3DPoint {
    /// Creates a `Trail3D` point.
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
            z: z.into(),
        }
    }

    /// Returns the point as `[x, y, z]`.
    pub fn as_array(self) -> [f32; 3] {
        [self.x.0, self.y.0, self.z.0]
    }
}

impl From<[f32; 3]> for Trail3DPoint {
    fn from([x, y, z]: [f32; 3]) -> Self {
        Self::new(x, y, z)
    }
}

impl From<(f32, f32, f32)> for Trail3DPoint {
    fn from((x, y, z): (f32, f32, f32)) -> Self {
        Self::new(x, y, z)
    }
}

impl From<Trail3DPoint> for [f32; 3] {
    fn from(value: Trail3DPoint) -> Self {
        value.as_array()
    }
}
