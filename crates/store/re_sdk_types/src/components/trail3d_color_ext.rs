use super::Trail3DColor;

impl Trail3DColor {
    /// Use a single solid color for the trail.
    pub fn solid(color: impl Into<crate::components::Color>) -> Self {
        Self(crate::datatypes::Trail3DColor::solid(color))
    }

    /// Map logged magnitudes through the selected colormap.
    pub fn magnitude(colormap: crate::components::Colormap) -> Self {
        Self(crate::datatypes::Trail3DColor::magnitude(colormap))
    }
}
