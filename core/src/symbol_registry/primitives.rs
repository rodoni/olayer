use serde::{Serialize, Deserialize};

/// RGBA colour for symbol primitives.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// Creates an opaque colour from red, green and blue channels.
    #[inline]
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Creates a colour with an explicit alpha channel.
    #[inline]
    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

/// Stroke descriptor for vector primitives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub color: Color,
    pub width: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dash_array: Option<Vec<f32>>,
}

impl Stroke {
    /// Creates a solid stroke with the given colour and width.
    #[inline]
    pub fn new(color: Color, width: f32) -> Self {
        Self {
            color,
            width,
            dash_array: None,
        }
    }

    /// Creates a stroke with a dash pattern.
    #[inline]
    pub fn with_dash_array(color: Color, width: f32, dash_array: Vec<f32>) -> Self {
        Self {
            color,
            width,
            dash_array: Some(dash_array),
        }
    }
}

/// Vector primitive for procedural symbol drawing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SymbolPrimitive {
    Path {
        commands: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        fill: Option<Color>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stroke: Option<Stroke>,
    },
    Circle {
        cx: f64,
        cy: f64,
        r: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        fill: Option<Color>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stroke: Option<Stroke>,
    },
    Text {
        content: String,
        offset_x: f64,
        offset_y: f64,
        font_size: f32,
        color: Color,
    },
}

/// A fully-resolved symbol ready for rendering or rasterisation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedSymbol {
    pub symbol_id: String,
    pub primitives: Vec<SymbolPrimitive>,
    /// Bounding box: `(min_x, min_y, max_x, max_y)`.
    pub bbox: (f64, f64, f64, f64),
    /// Anchor point: `(x, y)`.
    pub anchor: (f64, f64),
}
