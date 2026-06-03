use std::collections::HashMap;

/// Registry of parsed SLD layers and their style rules.
#[derive(Debug, Clone, Default)]
pub struct StyleRegistry {
    pub layers: HashMap<String, Vec<RuleStyle>>,
}

impl StyleRegistry {
    /// Returns the rules for a given layer whose scale range contains `scale_denominator`.
    #[inline]
    pub fn get_applicable_rules(&self, layer_name: &str, scale_denominator: f64) -> Vec<RuleStyle> {
        self.layers
            .get(layer_name)
            .map(|rules| {
                rules
                    .iter()
                    .filter(|rule| {
                        let min_ok = rule.min_scale.is_none_or(|min| scale_denominator >= min);
                        let max_ok = rule.max_scale.is_none_or(|max| scale_denominator <= max);
                        min_ok && max_ok
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// A single SLD rule, grouping scale filters and symbolizers.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleStyle {
    pub name: String,
    pub min_scale: Option<f64>,
    pub max_scale: Option<f64>,
    pub stroke: Option<StrokeStyle>,
    pub fill: Option<FillStyle>,
    pub text: Option<TextStyle>,
    pub point: Option<PointStyle>,
}

/// Line (stroke) symbolizer properties.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeStyle {
    pub color: String,
    pub width: f32,
    pub dash_array: Option<Vec<f32>>,
}

/// Polygon fill symbolizer properties.
#[derive(Debug, Clone, PartialEq)]
pub struct FillStyle {
    pub color: String,
    pub opacity: f32,
}

/// Text label symbolizer properties.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub label_expression: String,
    pub font_family: String,
    pub font_size: f32,
    pub fill_color: String,
}

/// Point / marker symbolizer properties.
#[derive(Debug, Clone, PartialEq)]
pub struct PointStyle {
    pub well_known_name: String,
    pub size: f32,
    pub fill_color: Option<String>,
    pub stroke_color: Option<String>,
    pub stroke_width: Option<f32>,
}
