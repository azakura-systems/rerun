use super::Quad3D;
use crate::{Quad3DFrame, Quad3DWithPose};

impl Quad3D {
    /// Creates a Quad3D frame with native Transform3D pose components.
    pub fn from_frame(frame: impl Into<Quad3DFrame>) -> Quad3DWithPose {
        Quad3DWithPose::from_frame(frame)
    }

    /// Updates only the native Transform3D pose components and Quad3D actuator angles.
    pub fn update_frame(frame: impl Into<Quad3DFrame>) -> Quad3DWithPose {
        Quad3DWithPose::from_frame(frame)
    }
}
