use crate::{AsComponents, SerializedComponentBatch};

/// Per-frame Quad3D pose and actuator state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad3DFrame {
    /// Translation in the parent coordinate system.
    pub translation: crate::components::Translation3D,

    /// Rotation in the parent coordinate system.
    pub quaternion: crate::components::RotationQuat,

    /// Quadrotor actuator angles in radians.
    pub thetas: crate::datatypes::Quad3DThetas,
}

/// A Quad3D loggable that also writes the native Transform3D pose components.
#[derive(Clone, Debug, PartialEq)]
pub struct Quad3DWithPose {
    quad: crate::archetypes::Quad3D,
    pose: crate::archetypes::Transform3D,
}

impl Quad3DWithPose {
    /// Creates a Quad3D frame with native Transform3D pose components.
    pub fn from_frame(frame: impl Into<Quad3DFrame>) -> Self {
        let frame = frame.into();
        Self {
            quad: crate::archetypes::Quad3D::new(frame.thetas),
            pose: crate::archetypes::Transform3D::update_fields()
                .with_translation(frame.translation)
                .with_quaternion(frame.quaternion),
        }
    }

    /// Sets the pose and actuator state.
    pub fn with_frame(self, frame: impl Into<Quad3DFrame>) -> Self {
        let frame = frame.into();
        let mut this = self;
        this.pose = this
            .pose
            .with_translation(frame.translation)
            .with_quaternion(frame.quaternion);
        this.with_thetas(frame.thetas)
    }

    /// Sets quadrotor actuator angles in radians.
    pub fn with_thetas(mut self, thetas: impl Into<crate::components::Quad3DThetas>) -> Self {
        self.quad = self.quad.with_thetas(thetas);
        self
    }

    /// Sets the GLB model filename.
    pub fn with_model(mut self, model: impl Into<crate::components::Quad3DModel>) -> Self {
        self.quad = self.quad.with_model(model);
        self
    }

    /// Sets the label.
    pub fn with_label(mut self, label: impl Into<crate::components::Text>) -> Self {
        self.quad = self.quad.with_label(label);
        self
    }

    /// Sets whether the label should be shown.
    pub fn with_show_label(mut self, show_label: impl Into<crate::components::ShowLabels>) -> Self {
        self.quad = self.quad.with_show_label(show_label);
        self
    }
}

impl AsComponents for Quad3DWithPose {
    fn as_serialized_batches(&self) -> Vec<SerializedComponentBatch> {
        let mut batches = self.pose.as_serialized_batches();
        batches.extend(self.quad.as_serialized_batches());
        batches
    }
}
