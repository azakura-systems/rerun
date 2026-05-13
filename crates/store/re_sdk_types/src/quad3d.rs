use crate::{AsComponents, SerializedComponentBatch};

/// A `Quad3D` loggable that also writes native `Transform3D` components when configured.
#[derive(Clone, Debug, PartialEq)]
pub struct Quad3D {
    quad: crate::archetypes::Quad3D,
    transform: crate::archetypes::Transform3D,
}

impl Quad3D {
    /// Creates a `Quad3D` from quadrotor actuator angles in radians.
    pub fn from_thetas(thetas: impl Into<crate::datatypes::Quad3DThetas>) -> Self {
        Self {
            quad: crate::archetypes::Quad3D::new(thetas),
            transform: crate::archetypes::Transform3D::update_fields(),
        }
    }

    /// Updates only specific `Quad3D` and `Transform3D` fields.
    pub fn update_fields() -> Self {
        Self {
            quad: crate::archetypes::Quad3D::update_fields(),
            transform: crate::archetypes::Transform3D::update_fields(),
        }
    }

    /// Clears all `Quad3D` and `Transform3D` fields.
    pub fn clear_fields() -> Self {
        Self {
            quad: crate::archetypes::Quad3D::clear_fields(),
            transform: crate::archetypes::Transform3D::clear_fields(),
        }
    }

    /// Sets quadrotor actuator angles in radians.
    pub fn with_thetas(mut self, thetas: impl Into<crate::datatypes::Quad3DThetas>) -> Self {
        self.quad = self.quad.with_thetas(thetas);
        self
    }

    /// Sets the native `Transform3D` translation.
    pub fn with_translation(mut self, translation: impl Into<crate::datatypes::Vec3D>) -> Self {
        self.transform = self.transform.with_translation(translation.into());
        self
    }

    /// Sets the native `Transform3D` quaternion rotation.
    pub fn with_quaternion(mut self, quaternion: impl Into<crate::datatypes::Quaternion>) -> Self {
        self.transform = self.transform.with_quaternion(quaternion.into());
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

impl AsComponents for Quad3D {
    fn as_serialized_batches(&self) -> Vec<SerializedComponentBatch> {
        let mut batches = self.transform.as_serialized_batches();
        batches.extend(self.quad.as_serialized_batches());
        batches
    }
}
