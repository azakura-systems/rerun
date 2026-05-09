use super::Trail3DColor;
use crate::reflection::Enum as _;

const SOLID_MODE: u8 = 1;
const MAGNITUDE_MODE: u8 = 2;
const DEFAULT_SOLID_COLOR: crate::datatypes::Rgba32 =
    crate::datatypes::Rgba32::from_rgb(80, 220, 140);

impl Trail3DColor {
    /// Use a single solid color for the trail.
    pub fn solid(color: impl Into<crate::components::Color>) -> Self {
        Self {
            mode: SOLID_MODE,
            color: color.into().0.to_u32(),
            colormap: crate::components::Colormap::default() as u8,
        }
    }

    /// Map logged magnitudes through the selected colormap.
    pub fn magnitude(colormap: crate::components::Colormap) -> Self {
        Self {
            mode: MAGNITUDE_MODE,
            color: DEFAULT_SOLID_COLOR.to_u32(),
            colormap: colormap as u8,
        }
    }

    /// Returns true if the trail should use a solid color.
    pub fn is_solid(&self) -> bool {
        self.mode != MAGNITUDE_MODE
    }

    /// Returns true if the trail should use magnitude colormapping.
    pub fn is_magnitude(&self) -> bool {
        self.mode == MAGNITUDE_MODE
    }

    /// Returns the configured colormap, falling back to the default colormap if invalid.
    pub fn colormap(&self) -> crate::components::Colormap {
        crate::components::Colormap::try_from_integer(self.colormap).unwrap_or_default()
    }
}

impl From<crate::components::Color> for Trail3DColor {
    #[inline]
    fn from(color: crate::components::Color) -> Self {
        Self::solid(color)
    }
}
