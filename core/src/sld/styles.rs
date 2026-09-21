use std::collections::HashMap;

/// Registry of parsed SLD layers and their style rules.
#[derive(Debug, Clone, Default)]
pub struct StyleRegistry {
    pub layers: HashMap<String, Vec<RuleStyle>>,
}

impl StyleRegistry {
    /// Creates a new empty `StyleRegistry`.
    #[inline]
    pub fn new() -> Self {
        Self {
            layers: HashMap::new(),
        }
    }

    /// Returns the rules for a given layer whose scale range contains `scale_denominator`.
    #[inline]
    pub fn get_applicable_rules(&self, layer_name: &str, scale_denominator: f64) -> Vec<RuleStyle> {
        self.applicable_rules(layer_name, scale_denominator)
            .cloned()
            .collect()
    }

    /// Iterates over applicable rules without cloning style data.
    #[inline]
    pub fn applicable_rules(
        &self,
        layer_name: &str,
        scale_denominator: f64,
    ) -> impl Iterator<Item = &RuleStyle> {
        self.layers
            .get(layer_name)
            .into_iter()
            .flat_map(|rules| rules.iter())
            .filter(move |rule| {
                scale_denominator.is_finite()
                    && scale_denominator >= 0.0
                    && rule.min_scale.is_none_or(|min| scale_denominator >= min)
                    && rule.max_scale.is_none_or(|max| scale_denominator <= max)
            })
    }
}

/// A single SLD rule, grouping scale filters and symbolizers.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleStyle {
    pub(crate) name: String,
    pub(crate) min_scale: Option<f64>,
    pub(crate) max_scale: Option<f64>,
    pub(crate) stroke: Option<StrokeStyle>,
    pub(crate) fill: Option<FillStyle>,
    pub(crate) text: Option<TextStyle>,
    pub(crate) point: Option<PointStyle>,
}

/// Line (stroke) symbolizer properties.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeStyle {
    pub(crate) color: String,
    pub(crate) width: f32,
    pub(crate) dash_array: Option<Vec<f32>>,
}

/// Polygon fill symbolizer properties.
#[derive(Debug, Clone, PartialEq)]
pub struct FillStyle {
    pub(crate) color: String,
    pub(crate) opacity: f32,
}

/// Text label symbolizer properties.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub(crate) label_expression: String,
    pub(crate) font_family: String,
    pub(crate) font_size: f32,
    pub(crate) fill_color: String,
}

/// Point / marker symbolizer properties.
#[derive(Debug, Clone, PartialEq)]
pub struct PointStyle {
    pub(crate) well_known_name: String,
    pub(crate) size: f32,
    pub(crate) fill_color: Option<String>,
    pub(crate) stroke_color: Option<String>,
    pub(crate) stroke_width: Option<f32>,
}
