use super::Quad3DThetas;

impl Quad3DThetas {
    /// Creates quadrotor actuator angles in radians.
    pub fn new(fl: f32, fr: f32, rl: f32, rr: f32) -> Self {
        Self {
            fl: fl.into(),
            fr: fr.into(),
            rl: rl.into(),
            rr: rr.into(),
        }
    }

    /// Returns the angles as `[fl, fr, rl, rr]`.
    pub fn as_array(self) -> [f32; 4] {
        [self.fl.0, self.fr.0, self.rl.0, self.rr.0]
    }
}

impl From<[f32; 4]> for Quad3DThetas {
    fn from([fl, fr, rl, rr]: [f32; 4]) -> Self {
        Self::new(fl, fr, rl, rr)
    }
}

impl From<(f32, f32, f32, f32)> for Quad3DThetas {
    fn from((fl, fr, rl, rr): (f32, f32, f32, f32)) -> Self {
        Self::new(fl, fr, rl, rr)
    }
}

impl From<Quad3DThetas> for [f32; 4] {
    fn from(value: Quad3DThetas) -> Self {
        value.as_array()
    }
}
