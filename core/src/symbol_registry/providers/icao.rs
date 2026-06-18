use crate::sld::StyleRegistry;
use crate::symbol_registry::errors::SymbologyError;
use crate::symbol_registry::primitives::{Color, ResolvedSymbol, Stroke, SymbolPrimitive};
use crate::symbol_registry::providers::SymbologyProvider;

/// ICAO civil aviation navaid type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavaidType {
    Vor,
    VorDme,
    VorTac,
    Dme,
    Ndb,
    Tacan,
    Airport,
    Heliport,
    Waypoint,
    Intersection,
    RunwayThreshold,
}

impl NavaidType {
    /// Parses the navaid type from a symbol code.
    fn from_code(code: &str) -> Option<Self> {
        let lower = code.to_ascii_lowercase();
        if lower.contains("vortac") || lower.contains("vor_tac") {
            return Some(NavaidType::VorTac);
        }
        if lower.contains("vordme") || lower.contains("vor_dme") {
            return Some(NavaidType::VorDme);
        }
        if lower.contains("vor") {
            return Some(NavaidType::Vor);
        }
        if lower.contains("tacan") {
            return Some(NavaidType::Tacan);
        }
        if lower.contains("dme") {
            return Some(NavaidType::Dme);
        }
        if lower.contains("ndb") {
            return Some(NavaidType::Ndb);
        }
        if lower.contains("heliport") || lower.contains("heli") {
            return Some(NavaidType::Heliport);
        }
        if lower.contains("airport") || lower.contains("aerodrome") || lower.contains("ad:") {
            return Some(NavaidType::Airport);
        }
        if lower.contains("waypoint") || lower.contains("wpt") {
            return Some(NavaidType::Waypoint);
        }
        if lower.contains("intersection") || lower.contains("int:") {
            return Some(NavaidType::Intersection);
        }
        if lower.contains("runway") || lower.contains("threshold") {
            return Some(NavaidType::RunwayThreshold);
        }
        None
    }
}

/// Procedural ICAO civil aviation symbology provider.
///
/// Generates standard aviation navaid symbols (VOR, NDB, DME, TACAN, airport,
/// waypoint, etc.) using ICAO Annex 4 / DOC 8697 visual conventions.
///
/// Codes are prefixed with `icao:` (e.g. `icao:vor`, `icao:ndb`, `icao:vordme`,
/// `icao:airport`, `icao:waypoint`).
pub struct IcaoProvider {
    provider_name: &'static str,
}

impl IcaoProvider {
    /// Creates a new ICAO symbology provider.
    #[inline]
    pub fn new() -> Self {
        Self {
            provider_name: "ICAO",
        }
    }

    /// Builds the symbol primitives for a given navaid type.
    fn build_symbol(navaid: NavaidType) -> (Vec<SymbolPrimitive>, (f64, f64, f64, f64)) {
        let magenta = Color::rgb(180, 0, 180);
        let blue = Color::rgb(0, 80, 200);
        let black = Color::rgb(0, 0, 0);
        let white = Color::rgb(255, 255, 255);

        match navaid {
            NavaidType::Vor => {
                // VOR: hexagon with dot in center
                (
                    vec![
                        SymbolPrimitive::Path {
                            commands: "M 0,-14 L 12,-7 L 12,7 L 0,14 L -12,7 L -12,-7 Z".to_string(),
                            fill: Some(white),
                            stroke: Some(Stroke::new(blue, 2.0)),
                        },
                        SymbolPrimitive::Circle {
                            cx: 0.0,
                            cy: 0.0,
                            r: 2.0,
                            fill: Some(blue),
                            stroke: None,
                        },
                    ],
                    (-12.0, -14.0, 12.0, 14.0),
                )
            }
            NavaidType::VorDme => {
                // VOR-DME: hexagon + small rectangle (DME) adjacent
                (
                    vec![
                        SymbolPrimitive::Path {
                            commands: "M 0,-14 L 12,-7 L 12,7 L 0,14 L -12,7 L -12,-7 Z".to_string(),
                            fill: Some(white),
                            stroke: Some(Stroke::new(blue, 2.0)),
                        },
                        SymbolPrimitive::Circle {
                            cx: 0.0,
                            cy: 0.0,
                            r: 2.0,
                            fill: Some(blue),
                            stroke: None,
                        },
                        // DME box to the upper right
                        SymbolPrimitive::Path {
                            commands: "M 14,-12 L 20,-12 L 20,-6 L 14,-6 Z".to_string(),
                            fill: Some(white),
                            stroke: Some(Stroke::new(blue, 1.5)),
                        },
                    ],
                    (-12.0, -14.0, 20.0, 14.0),
                )
            }
            NavaidType::VorTac => {
                // VORTAC: hexagon + small triangle (TACAN) adjacent
                (
                    vec![
                        SymbolPrimitive::Path {
                            commands: "M 0,-14 L 12,-7 L 12,7 L 0,14 L -12,7 L -12,-7 Z".to_string(),
                            fill: Some(white),
                            stroke: Some(Stroke::new(blue, 2.0)),
                        },
                        SymbolPrimitive::Circle {
                            cx: 0.0,
                            cy: 0.0,
                            r: 2.0,
                            fill: Some(blue),
                            stroke: None,
                        },
                        // TACAN triangle to the upper right
                        SymbolPrimitive::Path {
                            commands: "M 14,-12 L 20,-12 L 17,-6 Z".to_string(),
                            fill: Some(white),
                            stroke: Some(Stroke::new(blue, 1.5)),
                        },
                    ],
                    (-12.0, -14.0, 20.0, 14.0),
                )
            }
            NavaidType::Dme => {
                // DME: small rectangle
                (
                    vec![SymbolPrimitive::Path {
                        commands: "M -8,-6 L 8,-6 L 8,6 L -8,6 Z".to_string(),
                        fill: Some(white),
                        stroke: Some(Stroke::new(blue, 2.0)),
                    }],
                    (-8.0, -6.0, 8.0, 6.0),
                )
            }
            NavaidType::Ndb => {
                // NDB: circle with annotation marks (pear shape)
                (
                    vec![
                        SymbolPrimitive::Circle {
                            cx: 0.0,
                            cy: 0.0,
                            r: 10.0,
                            fill: Some(white),
                            stroke: Some(Stroke::new(magenta, 2.0)),
                        },
                        // Inner dots representing the NDB mast
                        SymbolPrimitive::Circle {
                            cx: 0.0,
                            cy: 0.0,
                            r: 1.5,
                            fill: Some(magenta),
                            stroke: None,
                        },
                    ],
                    (-10.0, -10.0, 10.0, 10.0),
                )
            }
            NavaidType::Tacan => {
                // TACAN: circle with inscribed triangle
                (
                    vec![
                        SymbolPrimitive::Circle {
                            cx: 0.0,
                            cy: 0.0,
                            r: 12.0,
                            fill: Some(white),
                            stroke: Some(Stroke::new(blue, 2.0)),
                        },
                        SymbolPrimitive::Path {
                            commands: "M 0,-7 L 6,4 L -6,4 Z".to_string(),
                            fill: None,
                            stroke: Some(Stroke::new(blue, 1.5)),
                        },
                    ],
                    (-12.0, -12.0, 12.0, 12.0),
                )
            }
            NavaidType::Airport => {
                // Airport: circle with inscribed runway shape
                (
                    vec![
                        SymbolPrimitive::Circle {
                            cx: 0.0,
                            cy: 0.0,
                            r: 12.0,
                            fill: Some(white),
                            stroke: Some(Stroke::new(black, 2.0)),
                        },
                        // Runway strip
                        SymbolPrimitive::Path {
                            commands: "M -8,-1 L 8,-1 L 8,1 L -8,1 Z".to_string(),
                            fill: Some(black),
                            stroke: None,
                        },
                    ],
                    (-12.0, -12.0, 12.0, 12.0),
                )
            }
            NavaidType::Heliport => {
                // Heliport: circle with "H" symbol
                (
                    vec![
                        SymbolPrimitive::Circle {
                            cx: 0.0,
                            cy: 0.0,
                            r: 12.0,
                            fill: Some(white),
                            stroke: Some(Stroke::new(black, 2.0)),
                        },
                        SymbolPrimitive::Text {
                            content: "H".to_string(),
                            offset_x: -4.0,
                            offset_y: 4.0,
                            font_size: 12.0,
                            color: black,
                        },
                    ],
                    (-12.0, -12.0, 12.0, 12.0),
                )
            }
            NavaidType::Waypoint | NavaidType::Intersection => {
                // Waypoint: triangle (intersection marker)
                (
                    vec![SymbolPrimitive::Path {
                        commands: "M 0,-10 L 9,6 L -9,6 Z".to_string(),
                        fill: None,
                        stroke: Some(Stroke::new(black, 1.5)),
                    }],
                    (-9.0, -10.0, 9.0, 6.0),
                )
            }
            NavaidType::RunwayThreshold => {
                // Runway threshold: arrow pointing down (approach direction)
                (
                    vec![SymbolPrimitive::Path {
                        commands: "M -8,-10 L 8,-10 L 0,10 Z".to_string(),
                        fill: Some(white),
                        stroke: Some(Stroke::new(black, 2.0)),
                    }],
                    (-8.0, -10.0, 8.0, 10.0),
                )
            }
        }
    }
}

impl Default for IcaoProvider {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl SymbologyProvider for IcaoProvider {
    #[inline]
    fn name(&self) -> &str {
        self.provider_name
    }

    #[inline]
    fn can_resolve(&self, code: &str) -> bool {
        code.starts_with("icao:") && NavaidType::from_code(code).is_some()
    }

    fn resolve(&self, code: &str, _style: &StyleRegistry) -> Result<ResolvedSymbol, SymbologyError> {
        let navaid = NavaidType::from_code(code)
            .ok_or_else(|| SymbologyError::SymbolNotFound(code.to_string()))?;

        let (primitives, bbox) = Self::build_symbol(navaid);

        Ok(ResolvedSymbol {
            symbol_id: code.to_string(),
            primitives,
            bbox,
            anchor: (0.0, 0.0),
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sld::StyleRegistry;

    #[test]
    fn test_icao_provider_name() {
        let provider = IcaoProvider::new();
        assert_eq!(provider.name(), "ICAO");
    }

    #[test]
    fn test_can_resolve_icao_prefix() {
        let provider = IcaoProvider::new();
        assert!(provider.can_resolve("icao:vor"));
        assert!(provider.can_resolve("icao:ndb"));
        assert!(provider.can_resolve("icao:vordme"));
        assert!(provider.can_resolve("icao:airport"));
        assert!(provider.can_resolve("icao:waypoint"));
        assert!(provider.can_resolve("icao:intersection"));
        assert!(provider.can_resolve("icao:heliport"));
        assert!(provider.can_resolve("icao:runway"));
        assert!(!provider.can_resolve("nato:friend"));
        assert!(!provider.can_resolve("icao:unknown_type"));
    }

    #[test]
    fn test_resolve_vor() {
        let provider = IcaoProvider::new();
        let style = StyleRegistry::default();
        let symbol = provider.resolve("icao:vor", &style).unwrap();

        assert_eq!(symbol.symbol_id, "icao:vor");
        // Hexagon frame + center dot = 2 primitives
        assert_eq!(symbol.primitives.len(), 2);
        assert_eq!(symbol.bbox, (-12.0, -14.0, 12.0, 14.0));
    }

    #[test]
    fn test_resolve_vordme() {
        let provider = IcaoProvider::new();
        let style = StyleRegistry::default();
        let symbol = provider.resolve("icao:vordme", &style).unwrap();

        // Hexagon + center dot + DME box = 3 primitives
        assert_eq!(symbol.primitives.len(), 3);
    }

    #[test]
    fn test_resolve_vortac() {
        let provider = IcaoProvider::new();
        let style = StyleRegistry::default();
        let symbol = provider.resolve("icao:vortac", &style).unwrap();

        // Hexagon + center dot + TACAN triangle = 3 primitives
        assert_eq!(symbol.primitives.len(), 3);
    }

    #[test]
    fn test_resolve_ndb() {
        let provider = IcaoProvider::new();
        let style = StyleRegistry::default();
        let symbol = provider.resolve("icao:ndb", &style).unwrap();

        // Circle + inner dot = 2 primitives
        assert_eq!(symbol.primitives.len(), 2);

        // NDB uses magenta colour
        if let SymbolPrimitive::Circle { stroke, .. } = &symbol.primitives[0] {
            assert_eq!(stroke.as_ref().unwrap().color, Color::rgb(180, 0, 180));
        } else {
            panic!("First primitive should be a Circle");
        }
    }

    #[test]
    fn test_resolve_tacan() {
        let provider = IcaoProvider::new();
        let style = StyleRegistry::default();
        let symbol = provider.resolve("icao:tacan", &style).unwrap();

        // Circle + inscribed triangle = 2 primitives
        assert_eq!(symbol.primitives.len(), 2);
    }

    #[test]
    fn test_resolve_airport() {
        let provider = IcaoProvider::new();
        let style = StyleRegistry::default();
        let symbol = provider.resolve("icao:airport", &style).unwrap();

        // Circle + runway strip = 2 primitives
        assert_eq!(symbol.primitives.len(), 2);
    }

    #[test]
    fn test_resolve_heliport() {
        let provider = IcaoProvider::new();
        let style = StyleRegistry::default();
        let symbol = provider.resolve("icao:heliport", &style).unwrap();

        // Circle + "H" text = 2 primitives
        assert_eq!(symbol.primitives.len(), 2);

        // Second primitive should be Text with content "H"
        if let SymbolPrimitive::Text { content, .. } = &symbol.primitives[1] {
            assert_eq!(content, "H");
        } else {
            panic!("Second primitive should be a Text with 'H'");
        }
    }

    #[test]
    fn test_resolve_waypoint() {
        let provider = IcaoProvider::new();
        let style = StyleRegistry::default();
        let symbol = provider.resolve("icao:waypoint", &style).unwrap();

        // Triangle = 1 primitive
        assert_eq!(symbol.primitives.len(), 1);

        if let SymbolPrimitive::Path { stroke, fill, .. } = &symbol.primitives[0] {
            assert!(fill.is_none(), "Waypoint should be unfilled");
            assert_eq!(stroke.as_ref().unwrap().color, Color::rgb(0, 0, 0));
        } else {
            panic!("Should be a Path primitive");
        }
    }

    #[test]
    fn test_resolve_intersection() {
        let provider = IcaoProvider::new();
        let style = StyleRegistry::default();
        let symbol = provider.resolve("icao:intersection", &style).unwrap();

        // Same as waypoint: triangle = 1 primitive
        assert_eq!(symbol.primitives.len(), 1);
    }

    #[test]
    fn test_resolve_runway_threshold() {
        let provider = IcaoProvider::new();
        let style = StyleRegistry::default();
        let symbol = provider.resolve("icao:runway", &style).unwrap();

        // Arrow = 1 primitive
        assert_eq!(symbol.primitives.len(), 1);
    }

    #[test]
    fn test_resolve_dme() {
        let provider = IcaoProvider::new();
        let style = StyleRegistry::default();
        let symbol = provider.resolve("icao:dme", &style).unwrap();

        // Rectangle = 1 primitive
        assert_eq!(symbol.primitives.len(), 1);
    }

    #[test]
    fn test_unknown_navaid_returns_error() {
        let provider = IcaoProvider::new();
        let style = StyleRegistry::default();
        // "icao:foo" has the right prefix but no recognized navaid type
        let result = provider.resolve("icao:foo", &style);
        assert!(matches!(result, Err(SymbologyError::SymbolNotFound(_))));
    }

    #[test]
    fn test_default_impl() {
        let provider = IcaoProvider::default();
        assert_eq!(provider.name(), "ICAO");
    }

    #[test]
    fn test_registry_integration_with_nato() {
        use crate::symbol_registry::registry::SymbolRegistry;

        let mut registry = SymbolRegistry::new();
        registry.register_provider(Box::new(IcaoProvider::new()));

        let style = StyleRegistry::default();
        let vor = registry.resolve_symbol("icao:vor", &style).unwrap();
        assert_eq!(vor.primitives.len(), 2);

        let ndb = registry.resolve_symbol("icao:ndb", &style).unwrap();
        assert_eq!(ndb.primitives.len(), 2);

        // Non-ICAO code should fail
        let err = registry.resolve_symbol("nato:friend", &style);
        assert_eq!(err.unwrap_err(), SymbologyError::ProviderNotFound);
    }
}
