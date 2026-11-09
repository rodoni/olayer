//! Small, deterministic WFS property rules. This is not an SLD implementation.
use super::{invalid, WfsRenderError};
use olayer_map_core::FeatureCollection;
use serde_json::Value;

/// Normalized, straight-alpha RGBA, validated on construction.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(transparent)]
pub struct WfsRgba([f32; 4]);
impl Default for WfsRgba {
    fn default() -> Self {
        Self([0.0, 0.9, 0.46, 1.0])
    }
}
impl TryFrom<[f32; 4]> for WfsRgba {
    type Error = WfsRenderError;
    fn try_from(value: [f32; 4]) -> Result<Self, Self::Error> {
        if value
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        {
            Ok(Self(value))
        } else {
            Err(invalid("invalid WFS normalized RGBA"))
        }
    }
}
impl WfsRgba {
    pub fn components(self) -> [f32; 4] {
        self.0
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WfsVisibility {
    #[default]
    Visible,
    Hidden,
}

/// Complete paint: disabled channels are `None`; matching paints never cascade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WfsPaint {
    pub visibility: WfsVisibility,
    pub stroke: Option<WfsRgba>,
    pub fill: Option<WfsRgba>,
}
impl Default for WfsPaint {
    fn default() -> Self {
        Self {
            visibility: WfsVisibility::Visible,
            stroke: Some(WfsRgba::default()),
            fill: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WfsMatchPriority {
    #[default]
    FirstMatch,
    LastMatch,
}

/// JSON scalar equality. Numbers use finite binary64 equality, not coercion.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum WfsScalar {
    #[default]
    Null,
    Boolean(bool),
    String(String),
    Number(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub enum WfsPredicate {
    Exists { property: String },
    Equals { property: String, value: WfsScalar },
}
impl Default for WfsPredicate {
    fn default() -> Self {
        Self::Exists {
            property: String::new(),
        }
    }
}
impl WfsPredicate {
    fn property(&self) -> &str {
        match self {
            Self::Exists { property } | Self::Equals { property, .. } => property,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WfsStyleRule {
    pub predicate: WfsPredicate,
    pub paint: WfsPaint,
}

/// Validated ordered rules. Defaults to visible green outlines and no fill.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WfsStyleSheet {
    default_paint: WfsPaint,
    rules: Vec<WfsStyleRule>,
    priority: WfsMatchPriority,
}

fn number(value: f64) -> Result<f64, WfsRenderError> {
    if !value.is_finite() || (value.fract() == 0.0 && value.abs() > 9_007_199_254_740_991.0) {
        Err(invalid(
            "WFS equality number must be finite and integral values must be safe integers",
        ))
    } else {
        Ok(value)
    }
}

impl WfsStyleSheet {
    /// Creates a bounded sheet with complete paints.
    ///
    /// # Errors
    /// Rejects more than 64 rules, keys exceeding 256 UTF-8 bytes, strings
    /// exceeding 4096 bytes, and nonfinite/unsafe-integral numeric operands.
    pub fn new(
        default_paint: WfsPaint,
        rules: Vec<WfsStyleRule>,
        priority: WfsMatchPriority,
    ) -> Result<Self, WfsRenderError> {
        if rules.len() > 64 {
            return Err(invalid("WFS rule limit exceeded"));
        }
        for rule in &rules {
            if rule.predicate.property().len() > 256 {
                return Err(invalid("WFS property key limit exceeded"));
            }
            if let WfsPredicate::Equals { value, .. } = &rule.predicate {
                match value {
                    WfsScalar::Number(n) => {
                        number(*n)?;
                    }
                    WfsScalar::String(s) if s.len() > 4096 => {
                        return Err(invalid("WFS equality string limit exceeded"))
                    }
                    _ => {}
                }
            }
        }
        Ok(Self {
            default_paint,
            rules,
            priority,
        })
    }

    pub(super) fn evaluate(
        &self,
        collection: &FeatureCollection,
    ) -> Result<Vec<WfsPaint>, WfsRenderError> {
        if collection.features.len() > 100_000 {
            return Err(invalid("WFS feature limit exceeded"));
        }
        collection
            .features
            .iter()
            .map(|feature| {
                let mut selected = None;
                for rule in &self.rules {
                    let value = feature.properties.get(rule.predicate.property());
                    let matched = match &rule.predicate {
                        WfsPredicate::Exists { .. } => value.is_some(),
                        WfsPredicate::Equals {
                            value: expected, ..
                        } => match (value, expected) {
                            (Some(Value::Null), WfsScalar::Null) => true,
                            (Some(Value::Bool(a)), WfsScalar::Boolean(b)) => a == b,
                            (Some(Value::String(a)), WfsScalar::String(b)) => a == b,
                            (Some(Value::Number(a)), WfsScalar::Number(b)) => {
                                number(
                                    a.as_f64()
                                        .ok_or_else(|| invalid("invalid WFS numeric property"))?,
                                )? == *b
                            }
                            _ => false,
                        },
                    };
                    if matched
                        && (selected.is_none() || self.priority == WfsMatchPriority::LastMatch)
                    {
                        selected = Some(rule.paint);
                    }
                }
                Ok(selected.unwrap_or(self.default_paint))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn collection(properties: Value) -> FeatureCollection {
        crate::decode_wfs_feature_collection(serde_json::json!({"type":"FeatureCollection","features":[{"type":"Feature","geometry":null,"properties":properties}]}).to_string().as_bytes(), olayer_map_core::CoordinateReferenceSystem::epsg(4326).unwrap()).unwrap()
    }
    #[test]
    fn missing_null_strict_numbers_and_priority() {
        let hidden = WfsPaint {
            visibility: WfsVisibility::Hidden,
            ..Default::default()
        };
        let rules = vec![
            WfsStyleRule {
                predicate: WfsPredicate::Exists {
                    property: "x".into(),
                },
                paint: hidden,
            },
            WfsStyleRule {
                predicate: WfsPredicate::Equals {
                    property: "x".into(),
                    value: WfsScalar::Null,
                },
                paint: WfsPaint::default(),
            },
        ];
        let first = WfsStyleSheet::new(
            Default::default(),
            rules.clone(),
            WfsMatchPriority::FirstMatch,
        )
        .unwrap();
        let last =
            WfsStyleSheet::new(Default::default(), rules, WfsMatchPriority::LastMatch).unwrap();
        assert_eq!(
            first.evaluate(&collection(serde_json::json!({}))).unwrap()[0],
            WfsPaint::default()
        );
        assert_eq!(
            first
                .evaluate(&collection(serde_json::json!({"x":null})))
                .unwrap()[0],
            hidden
        );
        assert_eq!(
            last.evaluate(&collection(serde_json::json!({"x":null})))
                .unwrap()[0],
            WfsPaint::default()
        );
        let numeric = WfsStyleSheet::new(
            Default::default(),
            vec![WfsStyleRule {
                predicate: WfsPredicate::Equals {
                    property: "x".into(),
                    value: WfsScalar::Number(0.0),
                },
                paint: hidden,
            }],
            Default::default(),
        )
        .unwrap();
        assert_eq!(
            numeric
                .evaluate(&collection(serde_json::json!({"x":-0.0})))
                .unwrap()[0],
            hidden
        );
        for value in [
            serde_json::json!("0"),
            serde_json::json!(false),
            serde_json::json!([]),
            Value::Null,
        ] {
            assert_eq!(
                numeric
                    .evaluate(&collection(serde_json::json!({"x":value})))
                    .unwrap()[0],
                WfsPaint::default()
            );
        }
        assert!(numeric
            .evaluate(&collection(serde_json::json!({"x":9007199254740992_u64})))
            .is_err());
    }
    #[test]
    fn bounds_and_traits() {
        assert!(WfsRgba::try_from([f32::NAN; 4]).is_err());
        assert!(WfsRgba::try_from([1.1; 4]).is_err());
        assert!(WfsStyleSheet::new(
            Default::default(),
            vec![WfsStyleRule::default(); 65],
            Default::default()
        )
        .is_err());
        fn traits<T: Send + Sync + Clone + std::fmt::Debug + Default>() {}
        traits::<WfsStyleSheet>();
        traits::<WfsPaint>();
    }
}
