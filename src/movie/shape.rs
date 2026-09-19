//! Vector shapes drawn by a frame.
use super::Transform;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Shape {
    pub geometry: Geometry,
    pub styles: Option<ShapeStyle>,
    pub transform: Option<Transform>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Geometry {
    /// An SVG path.
    Path { d: String },
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        corner_radius: f32,
    },
    Ellipse {
        x: f32,
        y: f32,
        radius_x: f32,
        radius_y: f32,
    },
    /// Repeat the shapes of the previous frame.
    Keep,
}

/// An empty path, which is what a shape without a `type` means.
impl Default for Geometry {
    fn default() -> Self {
        Self::Path { d: String::new() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ShapeStyle {
    pub fill: Option<Rgba>,
    pub stroke: Option<Rgba>,
    pub stroke_width: f32,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
    pub miter_limit: f32,
    pub line_dash: [f32; 3],
}

/// Components in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}
