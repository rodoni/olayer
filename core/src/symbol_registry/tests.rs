use crate::sld::{FillStyle, RuleStyle, StrokeStyle, StyleRegistry};
use crate::symbol_registry::errors::SymbologyError;
use crate::symbol_registry::primitives::{Color, ResolvedSymbol, Stroke, SymbolPrimitive};
use crate::symbol_registry::providers::{DeclarativeProvider, SymbologyProvider};
use crate::symbol_registry::registry::SymbolRegistry;

struct TestProgProvider;

impl SymbologyProvider for TestProgProvider {
    #[inline]
    fn name(&self) -> &str {
        "TestProg"
    }

    #[inline]
    fn can_resolve(&self, code: &str) -> bool {
        code.starts_with("test:")
    }

    #[inline]
    fn resolve(&self, code: &str, _style: &StyleRegistry) -> Result<ResolvedSymbol, SymbologyError> {
        Ok(ResolvedSymbol {
            symbol_id: code.to_string(),
            primitives: vec![SymbolPrimitive::Circle {
                cx: 0.0,
                cy: 0.0,
                r: 10.0,
                fill: Some(Color::rgb(255, 0, 0)),
                stroke: Some(Stroke::new(Color::rgb(0, 0, 0), 1.0)),
            }],
            bbox: (-10.0, -10.0, 10.0, 10.0),
            anchor: (0.0, 0.0),
        })
    }
}

#[test]
fn test_programmatic_provider() {
    let style = StyleRegistry::default();
    let provider = TestProgProvider;
    assert!(provider.can_resolve("test:circle"));
    assert!(!provider.can_resolve("other:circle"));

    let resolved = provider.resolve("test:circle", &style).unwrap();
    assert_eq!(resolved.symbol_id, "test:circle");
    assert_eq!(resolved.primitives.len(), 1);

    if let SymbolPrimitive::Circle { r, .. } = &resolved.primitives[0] {
        assert_eq!(*r, 10.0);
    } else {
        panic!("Expected a Circle primitive");
    }
}

#[test]
fn test_declarative_provider_from_json() {
    let json = r#"{
        "library_name": "CivilSymbols",
        "symbols": {
            "civil:vor": {
                "bbox": [-15.0, -15.0, 15.0, 15.0],
                "anchor": [0.0, 0.0],
                "primitives": [
                    {
                        "type": "Path",
                        "commands": "M -10,-10 L 10,-10 Z",
                        "stroke": { "color": { "r": 0, "g": 0, "b": 255, "a": 255 }, "width": 2.0 }
                    },
                    {
                        "type": "Circle",
                        "cx": 0.0,
                        "cy": 0.0,
                        "r": 5.0,
                        "fill": { "r": 0, "g": 255, "b": 0, "a": 255 }
                    },
                    {
                        "type": "Text",
                        "content": "V",
                        "offset_x": 0.0,
                        "offset_y": 0.0,
                        "font_size": 12.0,
                        "color": { "r": 0, "g": 0, "b": 0, "a": 255 }
                    }
                ]
            }
        }
    }"#;

    let provider = DeclarativeProvider::from_json(json).unwrap();
    assert_eq!(provider.name(), "CivilSymbols");
    assert!(provider.can_resolve("civil:vor"));

    let style = StyleRegistry::default();
    let resolved = provider.resolve("civil:vor", &style).unwrap();
    assert_eq!(resolved.bbox, (-15.0, -15.0, 15.0, 15.0));
    assert_eq!(resolved.anchor, (0.0, 0.0));
    assert_eq!(resolved.primitives.len(), 3);

    match &resolved.primitives[0] {
        SymbolPrimitive::Path { commands, stroke, .. } => {
            assert_eq!(commands, "M -10,-10 L 10,-10 Z");
            assert_eq!(stroke.as_ref().unwrap().width, 2.0);
        }
        _ => panic!("First primitive should be a Path"),
    }

    match &resolved.primitives[1] {
        SymbolPrimitive::Circle { r, fill, .. } => {
            assert_eq!(*r, 5.0);
            assert_eq!(fill.as_ref().unwrap().g, 255);
        }
        _ => panic!("Second primitive should be a Circle"),
    }

    match &resolved.primitives[2] {
        SymbolPrimitive::Text { content, font_size, .. } => {
            assert_eq!(content, "V");
            assert_eq!(*font_size, 12.0);
        }
        _ => panic!("Third primitive should be a Text"),
    }
}

#[test]
fn test_registry_chaining_and_errors() {
    let mut registry = SymbolRegistry::new();
    registry.register_provider(Box::new(TestProgProvider));

    let json = r#"{
        "library_name": "CivilSymbols",
        "symbols": {
            "civil:vor": {
                "bbox": [-15.0, -15.0, 15.0, 15.0],
                "anchor": [0.0, 0.0],
                "primitives": []
            }
        }
    }"#;
    let dec_provider = DeclarativeProvider::from_json(json).unwrap();
    registry.register_provider(Box::new(dec_provider));

    let style = StyleRegistry::default();

    // Resolve via the first provider
    let sym1 = registry.resolve_symbol("test:hello", &style).unwrap();
    assert_eq!(sym1.symbol_id, "test:hello");

    // Resolve via the second provider
    let sym2 = registry.resolve_symbol("civil:vor", &style).unwrap();
    assert_eq!(sym2.symbol_id, "civil:vor");

    // Unknown symbol should fail
    let err = registry.resolve_symbol("unknown:item", &style);
    assert_eq!(err.unwrap_err(), SymbologyError::ProviderNotFound);
}

#[test]
fn test_invalid_json_format() {
    let broken_json = r#"{ "library_name": "Broken", "symbols": { "civil:vor": { "bbox": "not_a_bbox" } } }"#;
    let res = DeclarativeProvider::from_json(broken_json);
    assert!(matches!(res, Err(SymbologyError::InvalidFormat(_))));
}

#[test]
fn test_sld_style_override() {
    let mut registry = SymbolRegistry::new();

    let json = r#"{
        "library_name": "LayerTest",
        "symbols": {
            "layer:vor": {
                "bbox": [-10.0, -10.0, 10.0, 10.0],
                "anchor": [0.0, 0.0],
                "primitives": [
                    {
                        "type": "Circle",
                        "cx": 0.0,
                        "cy": 0.0,
                        "r": 5.0,
                        "fill": { "r": 255, "g": 255, "b": 255, "a": 255 },
                        "stroke": { "color": { "r": 0, "g": 0, "b": 0, "a": 255 }, "width": 1.0 }
                    }
                ]
            }
        }
    }"#;
    let provider = DeclarativeProvider::from_json(json).unwrap();
    registry.register_provider(Box::new(provider));

    // Build a StyleRegistry with a matching SLD rule for "layer:vor"
    let mut style = StyleRegistry::default();
    style.layers.insert(
        "layer:vor".to_string(),
        vec![RuleStyle {
            name: "OverrideRule".to_string(),
            min_scale: None,
            max_scale: None,
            stroke: Some(StrokeStyle {
                color: "#FF00FF".to_string(),
                width: 4.5,
                dash_array: Some(vec![10.0, 5.0]),
            }),
            fill: Some(FillStyle {
                color: "#FFFF00".to_string(),
                opacity: 0.5,
            }),
            text: None,
            point: None,
        }],
    );

    let resolved = registry.resolve_symbol("layer:vor", &style).unwrap();

    if let SymbolPrimitive::Circle { fill, stroke, .. } = &resolved.primitives[0] {
        // Fill should be yellow (#FFFF00) with alpha ~127 (0.5 * 255)
        let fill_color = fill.as_ref().unwrap();
        assert_eq!(fill_color.r, 255);
        assert_eq!(fill_color.g, 255);
        assert_eq!(fill_color.b, 0);
        assert_eq!(fill_color.a, 127);

        // Stroke should be magenta (#FF00FF) with width 4.5 and dash array
        let stroke_style = stroke.as_ref().unwrap();
        assert_eq!(stroke_style.color.r, 255);
        assert_eq!(stroke_style.color.g, 0);
        assert_eq!(stroke_style.color.b, 255);
        assert_eq!(stroke_style.color.a, 255);
        assert_eq!(stroke_style.width, 4.5);
        assert_eq!(stroke_style.dash_array.as_ref().unwrap(), &vec![10.0, 5.0]);
    } else {
        panic!("Expected a Circle primitive");
    }
}

#[test]
fn test_color_constructors() {
    let rgb = Color::rgb(10, 20, 30);
    assert_eq!(rgb.r, 10);
    assert_eq!(rgb.g, 20);
    assert_eq!(rgb.b, 30);
    assert_eq!(rgb.a, 255);

    let rgba = Color::rgba(10, 20, 30, 128);
    assert_eq!(rgba.a, 128);
}

#[test]
fn test_stroke_constructors() {
    let s1 = Stroke::new(Color::rgb(0, 0, 0), 2.0);
    assert!(s1.dash_array.is_none());
    assert_eq!(s1.width, 2.0);

    let s2 = Stroke::with_dash_array(Color::rgb(255, 0, 0), 1.5, vec![4.0, 2.0]);
    assert_eq!(s2.dash_array.as_ref().unwrap(), &vec![4.0, 2.0]);
}

#[test]
fn test_sld_no_matching_layer() {
    let mut registry = SymbolRegistry::new();

    let json = r#"{
        "library_name": "NoStyle",
        "symbols": {
            "ns:sym": {
                "bbox": [0.0, 0.0, 10.0, 10.0],
                "anchor": [0.0, 0.0],
                "primitives": [
                    {
                        "type": "Circle",
                        "cx": 5.0,
                        "cy": 5.0,
                        "r": 5.0,
                        "fill": { "r": 255, "g": 0, "b": 0, "a": 255 }
                    }
                ]
            }
        }
    }"#;
    let provider = DeclarativeProvider::from_json(json).unwrap();
    registry.register_provider(Box::new(provider));

    let style = StyleRegistry::default();
    let resolved = registry.resolve_symbol("ns:sym", &style).unwrap();

    // Symbol should remain unchanged because no SLD layer matches "ns:sym"
    if let SymbolPrimitive::Circle { fill, .. } = &resolved.primitives[0] {
        assert_eq!(fill.as_ref().unwrap(), &Color::rgb(255, 0, 0));
    } else {
        panic!("Expected a Circle");
    }
}

#[test]
fn test_sld_multiple_rules() {
    let mut registry = SymbolRegistry::new();

    let json = r#"{
        "library_name": "MultiRule",
        "symbols": {
            "mr:sym": {
                "bbox": [0.0, 0.0, 10.0, 10.0],
                "anchor": [0.0, 0.0],
                "primitives": [
                    {
                        "type": "Circle",
                        "cx": 5.0,
                        "cy": 5.0,
                        "r": 5.0,
                        "fill": { "r": 255, "g": 255, "b": 255, "a": 255 },
                        "stroke": { "color": { "r": 0, "g": 0, "b": 0, "a": 255 }, "width": 1.0 }
                    }
                ]
            }
        }
    }"#;
    let provider = DeclarativeProvider::from_json(json).unwrap();
    registry.register_provider(Box::new(provider));

    let mut style = StyleRegistry::default();
    style.layers.insert(
        "mr:sym".to_string(),
        vec![
            RuleStyle {
                name: "First".to_string(),
                min_scale: None,
                max_scale: None,
                stroke: Some(StrokeStyle {
                    color: "#FF0000".to_string(),
                    width: 2.0,
                    dash_array: None,
                }),
                fill: Some(FillStyle {
                    color: "#00FF00".to_string(),
                    opacity: 1.0,
                }),
                text: None,
                point: None,
            },
            RuleStyle {
                name: "Second".to_string(),
                min_scale: None,
                max_scale: None,
                stroke: Some(StrokeStyle {
                    color: "#0000FF".to_string(),
                    width: 4.0,
                    dash_array: None,
                }),
                fill: Some(FillStyle {
                    color: "#000000".to_string(),
                    opacity: 0.0,
                }),
                text: None,
                point: None,
            },
        ],
    );

    let resolved = registry.resolve_symbol("mr:sym", &style).unwrap();

    if let SymbolPrimitive::Circle { fill, stroke, .. } = &resolved.primitives[0] {
        // Rules are applied in order; the second rule wins for both fill and stroke
        let fill_color = fill.as_ref().unwrap();
        assert_eq!(fill_color.r, 0);
        assert_eq!(fill_color.g, 0);
        assert_eq!(fill_color.b, 0);
        assert_eq!(fill_color.a, 0);

        let stroke_style = stroke.as_ref().unwrap();
        assert_eq!(stroke_style.color.b, 255);
        assert_eq!(stroke_style.width, 4.0);
    } else {
        panic!("Expected a Circle");
    }
}

#[test]
fn test_sld_invalid_hex_color() {
    let mut registry = SymbolRegistry::new();

    let json = r#"{
        "library_name": "BadHex",
        "symbols": {
            "bh:sym": {
                "bbox": [0.0, 0.0, 10.0, 10.0],
                "anchor": [0.0, 0.0],
                "primitives": [
                    {
                        "type": "Circle",
                        "cx": 5.0,
                        "cy": 5.0,
                        "r": 5.0,
                        "fill": { "r": 255, "g": 0, "b": 0, "a": 255 },
                        "stroke": { "color": { "r": 0, "g": 255, "b": 0, "a": 255 }, "width": 1.0 }
                    }
                ]
            }
        }
    }"#;
    let provider = DeclarativeProvider::from_json(json).unwrap();
    registry.register_provider(Box::new(provider));

    let mut style = StyleRegistry::default();
    style.layers.insert(
        "bh:sym".to_string(),
        vec![RuleStyle {
            name: "Bad".to_string(),
            min_scale: None,
            max_scale: None,
            stroke: Some(StrokeStyle {
                color: "not_a_hex".to_string(),
                width: 99.0,
                dash_array: None,
            }),
            fill: Some(FillStyle {
                color: "also_not_hex".to_string(),
                opacity: 0.5,
            }),
            text: None,
            point: None,
        }],
    );

    let resolved = registry.resolve_symbol("bh:sym", &style).unwrap();

    if let SymbolPrimitive::Circle { fill, stroke, .. } = &resolved.primitives[0] {
        // Invalid hex colours are ignored; original values are preserved
        assert_eq!(fill.as_ref().unwrap(), &Color::rgb(255, 0, 0));
        let stroke_style = stroke.as_ref().unwrap();
        assert_eq!(stroke_style.color, Color::rgb(0, 255, 0));
        assert_eq!(stroke_style.width, 1.0);
    } else {
        panic!("Expected a Circle");
    }
}

#[test]
fn test_sld_text_unchanged() {
    let mut registry = SymbolRegistry::new();

    let json = r#"{
        "library_name": "TextLib",
        "symbols": {
            "txt:label": {
                "bbox": [0.0, 0.0, 10.0, 10.0],
                "anchor": [0.0, 0.0],
                "primitives": [
                    {
                        "type": "Text",
                        "content": "LABEL",
                        "offset_x": 0.0,
                        "offset_y": 0.0,
                        "font_size": 12.0,
                        "color": { "r": 0, "g": 0, "b": 0, "a": 255 }
                    }
                ]
            }
        }
    }"#;
    let provider = DeclarativeProvider::from_json(json).unwrap();
    registry.register_provider(Box::new(provider));

    let mut style = StyleRegistry::default();
    style.layers.insert(
        "txt:label".to_string(),
        vec![RuleStyle {
            name: "TextOverride".to_string(),
            min_scale: None,
            max_scale: None,
            stroke: Some(StrokeStyle {
                color: "#FF0000".to_string(),
                width: 5.0,
                dash_array: None,
            }),
            fill: Some(FillStyle {
                color: "#00FF00".to_string(),
                opacity: 1.0,
            }),
            text: None,
            point: None,
        }],
    );

    let resolved = registry.resolve_symbol("txt:label", &style).unwrap();

    if let SymbolPrimitive::Text { color, content, .. } = &resolved.primitives[0] {
        assert_eq!(content, "LABEL");
        // Text primitives are not affected by stroke/fill SLD rules
        assert_eq!(color, &Color::rgb(0, 0, 0));
    } else {
        panic!("Expected a Text primitive");
    }
}

#[test]
fn test_symbol_registry_default() {
    let registry = SymbolRegistry::default();
    let style = StyleRegistry::default();
    let res = registry.resolve_symbol("anything", &style);
    assert_eq!(res.unwrap_err(), SymbologyError::ProviderNotFound);
}

#[test]
fn test_symbology_error_display() {
    assert_eq!(
        SymbologyError::ProviderNotFound.to_string(),
        "No registered provider can resolve this code"
    );
    assert_eq!(
        SymbologyError::SymbolNotFound("foo:bar".to_string()).to_string(),
        "Symbol not found: foo:bar"
    );
    assert_eq!(
        SymbologyError::InvalidFormat("bad json".to_string()).to_string(),
        "Invalid format: bad json"
    );
}
