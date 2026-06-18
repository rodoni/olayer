use crate::sld::StyleRegistry;
use crate::symbol_registry::errors::SymbologyError;
use crate::symbol_registry::primitives::{Color, ResolvedSymbol, Stroke, SymbolPrimitive};
use crate::symbol_registry::providers::SymbologyProvider;

/// Affiliation (Standard Identity) derived from SIDC position 2 (1-indexed)
/// or position 1 in 10-char codes.
///
/// Colors follow MIL-STD-2525 / APP-6 conventions:
/// * **Friend** — blue
/// * **Hostile** — red
/// * **Neutral** — green
/// * **Unknown** — yellow
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Affiliation {
    Pending,
    Unknown,
    Friend,
    Neutral,
    Hostile,
    AssumedFriend,
    Other,
}

impl Affiliation {
    /// Returns the frame/border colour for this affiliation.
    #[inline]
    pub fn frame_color(self) -> Color {
        match self {
            Affiliation::Friend | Affiliation::AssumedFriend => Color::rgb(0, 100, 255),
            Affiliation::Hostile => Color::rgb(255, 0, 0),
            Affiliation::Neutral => Color::rgb(0, 200, 0),
            Affiliation::Unknown | Affiliation::Pending => Color::rgb(255, 220, 0),
            Affiliation::Other => Color::rgb(128, 128, 128),
        }
    }

    /// Returns the fill colour for the symbol body.
    #[inline]
    pub fn fill_color(self) -> Color {
        match self {
            Affiliation::Friend | Affiliation::AssumedFriend => Color::rgba(0, 100, 255, 80),
            Affiliation::Hostile => Color::rgba(255, 0, 0, 80),
            Affiliation::Neutral => Color::rgba(0, 200, 0, 80),
            Affiliation::Unknown | Affiliation::Pending => Color::rgba(255, 220, 0, 80),
            Affiliation::Other => Color::rgba(128, 128, 128, 80),
        }
    }
}

/// Battle dimension from SIDC position 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattleDimension {
    Space,
    Air,
    Ground,
    Land,
    Surface,
    Subsurface,
    Other,
}

/// A simplified NATO APP-6 / MIL-STD-2525 SIDC code parser.
///
/// The provider accepts SIDC codes with the prefix `nato:` or `mil:` followed by
/// either a 15-character alphanumeric code or a compact `affiliation:entity` token
/// such as `nato:friend:fighter` or `nato:hostile:armor`.
///
/// The provider generates the standard frame shape (circle for unknown, rectangle
/// for friend, diamond for hostile, square for neutral) plus a simple icon based on
/// the battle dimension.
pub struct NatoProvider {
    provider_name: &'static str,
}

impl NatoProvider {
    /// Creates a new NATO symbology provider.
    #[inline]
    pub fn new() -> Self {
        Self {
            provider_name: "NATO_APP6",
        }
    }

    /// Parses the affiliation from the SIDC code.
    ///
    /// Supports both compact tokens (`friend`, `hostile`, etc.) and the
    /// digit at position 2 of a standard 15-char SIDC.
    fn parse_affiliation(code: &str) -> Affiliation {
        // Compact form: nato:friend:fighter  /  mil:hostile:armor
        let lower = code.to_ascii_lowercase();
        if lower.contains("friend") {
            return Affiliation::Friend;
        }
        if lower.contains("hostile") || lower.contains("enemy") {
            return Affiliation::Hostile;
        }
        if lower.contains("neutral") {
            return Affiliation::Neutral;
        }
        if lower.contains("unknown") {
            return Affiliation::Unknown;
        }
        if lower.contains("pending") {
            return Affiliation::Pending;
        }

        // Full 15-char SIDC: position 2 (index 1) encodes standard identity
        // Both digit (APP-6 style) and letter (MIL-STD-2525 style) codes are supported.
        let sidc = code.strip_prefix("nato:").or_else(|| code.strip_prefix("mil:")).unwrap_or(code);
        if sidc.len() >= 15 {
            let c = sidc.as_bytes()[1];
            return match c {
                b'0' | b'P' | b'p' => Affiliation::Pending,
                b'1' | b'U' | b'u' => Affiliation::Unknown,
                b'2' | b'F' | b'f' => Affiliation::Friend,
                b'3' | b'N' | b'n' => Affiliation::Neutral,
                b'4' | b'X' | b'x' => Affiliation::Other,
                b'5' | b'H' | b'h' | b'S' | b's' => Affiliation::Hostile,
                b'6' | b'A' | b'a' => Affiliation::AssumedFriend,
                _ => Affiliation::Other,
            };
        }

        Affiliation::Other
    }

    /// Parses the battle dimension from the SIDC code.
    fn parse_dimension(code: &str) -> BattleDimension {
        let lower = code.to_ascii_lowercase();
        if lower.contains("air") || lower.contains("fighter") || lower.contains("bomber") {
            return BattleDimension::Air;
        }
        if lower.contains("ground") || lower.contains("armor") || lower.contains("infantry") {
            return BattleDimension::Ground;
        }
        if lower.contains("surface") || lower.contains("ship") || lower.contains("naval") {
            return BattleDimension::Surface;
        }
        if lower.contains("subsurface") || lower.contains("submarine") {
            return BattleDimension::Subsurface;
        }
        if lower.contains("space") || lower.contains("satellite") {
            return BattleDimension::Space;
        }

        // Full 15-char SIDC: position 3 (index 2) encodes battle dimension
        // Both digit and letter codes are supported.
        let sidc = code.strip_prefix("nato:").or_else(|| code.strip_prefix("mil:")).unwrap_or(code);
        if sidc.len() >= 15 {
            let c = sidc.as_bytes()[2];
            return match c {
                b'0' | b'Z' | b'z' => BattleDimension::Space,
                b'1' | b'A' | b'a' => BattleDimension::Air,
                b'2' | b'4' | b'G' | b'g' | b'X' | b'x' => BattleDimension::Ground,
                b'3' | b'S' | b's' => BattleDimension::Land,
                b'5' | b'N' | b'n' => BattleDimension::Surface,
                b'6' | b'U' | b'u' => BattleDimension::Subsurface,
                _ => BattleDimension::Other,
            };
        }

        BattleDimension::Other
    }

    /// Builds the frame shape (border outline) for the given affiliation.
    /// Returns the SVG path commands and the bounding box.
    fn build_frame(affiliation: Affiliation) -> (&'static str, (f64, f64, f64, f64)) {
        match affiliation {
            Affiliation::Unknown | Affiliation::Pending => {
                // Cloud-like shape (simplified as a circle)
                ("M 0,-15 A 15,15 0 1 1 0,15 A 15,15 0 1 1 0,-15 Z", (-15.0, -15.0, 15.0, 15.0))
            }
            Affiliation::Friend | Affiliation::AssumedFriend => {
                // Rectangle
                ("M -15,-10 L 15,-10 L 15,10 L -15,10 Z", (-15.0, -10.0, 15.0, 10.0))
            }
            Affiliation::Hostile => {
                // Diamond
                ("M 0,-16 L 16,0 L 0,16 L -16,0 Z", (-16.0, -16.0, 16.0, 16.0))
            }
            Affiliation::Neutral => {
                // Square
                ("M -14,-14 L 14,-14 L 14,14 L -14,14 Z", (-14.0, -14.0, 14.0, 14.0))
            }
            Affiliation::Other => {
                // Rectangle (same as friend but different colour)
                ("M -15,-10 L 15,-10 L 15,10 L -15,10 Z", (-15.0, -10.0, 15.0, 10.0))
            }
        }
    }

    /// Builds the inner icon based on the battle dimension.
    fn build_icon(dimension: BattleDimension) -> SymbolPrimitive {
        let icon_stroke = Stroke::new(Color::rgb(0, 0, 0), 1.5);
        match dimension {
            BattleDimension::Air => {
                // Aircraft silhouette (simple triangle pointing up)
                SymbolPrimitive::Path {
                    commands: "M 0,-8 L 8,6 L 0,3 L -8,6 Z".to_string(),
                    fill: Some(Color::rgb(255, 255, 255)),
                    stroke: Some(icon_stroke),
                }
            }
            BattleDimension::Ground | BattleDimension::Land => {
                // Armor / ground unit (ellipse)
                SymbolPrimitive::Path {
                    commands: "M -8,0 A 8,5 0 1 0 8,0 A 8,5 0 1 0 -8,0 Z".to_string(),
                    fill: Some(Color::rgb(255, 255, 255)),
                    stroke: Some(icon_stroke),
                }
            }
            BattleDimension::Surface => {
                // Naval surface (half-circle hull + mast)
                SymbolPrimitive::Path {
                    commands: "M -8,2 A 8,8 0 0 1 8,2 M 0,2 L 0,-6 M -4,-6 L 4,-6".to_string(),
                    fill: None,
                    stroke: Some(icon_stroke),
                }
            }
            BattleDimension::Subsurface => {
                // Submarine (rounded body + conning tower)
                SymbolPrimitive::Path {
                    commands: "M -9,2 A 9,5 0 1 0 9,2 M -2,2 L -2,-4 L 2,-4 L 2,2".to_string(),
                    fill: None,
                    stroke: Some(icon_stroke),
                }
            }
            BattleDimension::Space => {
                // Satellite (circle + two panels)
                SymbolPrimitive::Path {
                    commands: "M -6,-4 L 6,-4 L 6,4 L -6,4 Z M -10,-2 L -6,-2 M -10,2 L -6,2 M 6,-2 L 10,-2 M 6,2 L 10,2".to_string(),
                    fill: None,
                    stroke: Some(icon_stroke),
                }
            }
            BattleDimension::Other => {
                // Generic marker (cross)
                SymbolPrimitive::Path {
                    commands: "M -6,0 L 6,0 M 0,-6 L 0,6".to_string(),
                    fill: None,
                    stroke: Some(icon_stroke),
                }
            }
        }
    }
}

impl Default for NatoProvider {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl SymbologyProvider for NatoProvider {
    #[inline]
    fn name(&self) -> &str {
        self.provider_name
    }

    #[inline]
    fn can_resolve(&self, code: &str) -> bool {
        code.starts_with("nato:") || code.starts_with("mil:")
    }

    fn resolve(&self, code: &str, _style: &StyleRegistry) -> Result<ResolvedSymbol, SymbologyError> {
        let affiliation = Self::parse_affiliation(code);
        let dimension = Self::parse_dimension(code);

        let (frame_path, bbox) = Self::build_frame(affiliation);
        let frame_color = affiliation.frame_color();
        let fill_color = affiliation.fill_color();

        let mut primitives = vec![
            SymbolPrimitive::Path {
                commands: frame_path.to_string(),
                fill: Some(fill_color),
                stroke: Some(Stroke::new(frame_color, 2.0)),
            },
            Self::build_icon(dimension),
        ];

        // For hostile units, add a direction-of-movement indicator line
        if affiliation == Affiliation::Hostile {
            primitives.push(SymbolPrimitive::Path {
                commands: "M 0,16 L 0,22".to_string(),
                fill: None,
                stroke: Some(Stroke::new(frame_color, 1.5)),
            });
        }

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
    fn test_nato_provider_name() {
        let provider = NatoProvider::new();
        assert_eq!(provider.name(), "NATO_APP6");
    }

    #[test]
    fn test_can_resolve_nato_prefix() {
        let provider = NatoProvider::new();
        assert!(provider.can_resolve("nato:friend:fighter"));
        assert!(provider.can_resolve("mil:hostile:armor"));
        assert!(!provider.can_resolve("civil:vor"));
        assert!(!provider.can_resolve("test:foo"));
    }

    #[test]
    fn test_resolve_friend_fighter() {
        let provider = NatoProvider::new();
        let style = StyleRegistry::default();
        let symbol = provider.resolve("nato:friend:fighter", &style).unwrap();

        assert_eq!(symbol.symbol_id, "nato:friend:fighter");
        // Frame (rectangle) + icon (air) = 2 primitives
        assert_eq!(symbol.primitives.len(), 2);

        // Frame should be a Path with blue stroke (friend)
        if let SymbolPrimitive::Path { stroke, .. } = &symbol.primitives[0] {
            assert_eq!(stroke.as_ref().unwrap().color, Color::rgb(0, 100, 255));
        } else {
            panic!("First primitive should be the frame Path");
        }
    }

    #[test]
    fn test_resolve_hostile_armor() {
        let provider = NatoProvider::new();
        let style = StyleRegistry::default();
        let symbol = provider.resolve("mil:hostile:armor", &style).unwrap();

        // Frame (diamond) + icon (ground) + direction line = 3 primitives (hostile gets direction line)
        assert_eq!(symbol.primitives.len(), 3);

        // Frame should be a Path with red stroke (hostile)
        if let SymbolPrimitive::Path { stroke, .. } = &symbol.primitives[0] {
            assert_eq!(stroke.as_ref().unwrap().color, Color::rgb(255, 0, 0));
        } else {
            panic!("First primitive should be the frame Path");
        }
    }

    #[test]
    fn test_resolve_neutral_ship() {
        let provider = NatoProvider::new();
        let style = StyleRegistry::default();
        let symbol = provider.resolve("nato:neutral:ship", &style).unwrap();

        // Frame (square) + icon (surface) = 2 primitives
        assert_eq!(symbol.primitives.len(), 2);

        // Frame should be green (neutral)
        if let SymbolPrimitive::Path { stroke, .. } = &symbol.primitives[0] {
            assert_eq!(stroke.as_ref().unwrap().color, Color::rgb(0, 200, 0));
        } else {
            panic!("First primitive should be the frame Path");
        }
    }

    #[test]
    fn test_resolve_unknown_air() {
        let provider = NatoProvider::new();
        let style = StyleRegistry::default();
        let symbol = provider.resolve("nato:unknown:air", &style).unwrap();

        // Unknown uses circle frame; yellow colour
        if let SymbolPrimitive::Path { stroke, .. } = &symbol.primitives[0] {
            assert_eq!(stroke.as_ref().unwrap().color, Color::rgb(255, 220, 0));
        } else {
            panic!("First primitive should be the frame Path");
        }
    }

    #[test]
    fn test_resolve_submarine() {
        let provider = NatoProvider::new();
        let style = StyleRegistry::default();
        let symbol = provider.resolve("nato:hostile:submarine", &style).unwrap();

        // Hostile subsurface: frame (diamond) + icon (submarine) + direction line = 3
        assert_eq!(symbol.primitives.len(), 3);

        // Verify the icon is a Path (submarine silhouette)
        match &symbol.primitives[1] {
            SymbolPrimitive::Path { commands, .. } => {
                assert!(commands.contains("A 9,5"), "Submarine icon should have an arc");
            }
            _ => panic!("Second primitive should be the submarine icon Path"),
        }
    }

    #[test]
    fn test_resolve_satellite() {
        let provider = NatoProvider::new();
        let style = StyleRegistry::default();
        let symbol = provider.resolve("nato:friend:satellite", &style).unwrap();

        // Friend space: frame (rectangle) + icon (satellite) = 2
        assert_eq!(symbol.primitives.len(), 2);
    }

    #[test]
    fn test_full_15char_sidc_friend() {
        let provider = NatoProvider::new();
        let style = StyleRegistry::default();
        // SIDC: position 2 = 'F' (Friend), position 3 = 'A' (Air)
        // 15-char code: S F A P U C I - - - - A - - -
        let symbol = provider.resolve("nato:SFAPUCI------A--", &style).unwrap();
        assert_eq!(symbol.primitives.len(), 2);

        if let SymbolPrimitive::Path { stroke, .. } = &symbol.primitives[0] {
            assert_eq!(stroke.as_ref().unwrap().color, Color::rgb(0, 100, 255));
        } else {
            panic!("Frame should be a Path");
        }
    }

    #[test]
    fn test_full_15char_sidc_hostile() {
        let provider = NatoProvider::new();
        let style = StyleRegistry::default();
        // SIDC: position 2 = 'H' (Hostile), position 3 = 'N' (Surface)
        // 15-char code: S H N P - - - - - - - - - - -
        let symbol = provider.resolve("mil:SHNP-----------", &style).unwrap();
        // Hostile always gets direction line
        assert_eq!(symbol.primitives.len(), 3);

        if let SymbolPrimitive::Path { stroke, .. } = &symbol.primitives[0] {
            assert_eq!(stroke.as_ref().unwrap().color, Color::rgb(255, 0, 0));
        } else {
            panic!("Frame should be a Path");
        }
    }

    #[test]
    fn test_default_impl() {
        let provider = NatoProvider::default();
        assert_eq!(provider.name(), "NATO_APP6");
    }

    #[test]
    fn test_affiliation_colors() {
        assert_eq!(Affiliation::Friend.frame_color(), Color::rgb(0, 100, 255));
        assert_eq!(Affiliation::Hostile.frame_color(), Color::rgb(255, 0, 0));
        assert_eq!(Affiliation::Neutral.frame_color(), Color::rgb(0, 200, 0));
        assert_eq!(Affiliation::Unknown.frame_color(), Color::rgb(255, 220, 0));
    }

    #[test]
    fn test_registry_integration() {
        use crate::symbol_registry::registry::SymbolRegistry;

        let mut registry = SymbolRegistry::new();
        registry.register_provider(Box::new(NatoProvider::new()));

        let style = StyleRegistry::default();
        let symbol = registry.resolve_symbol("nato:friend:fighter", &style).unwrap();
        assert_eq!(symbol.primitives.len(), 2);

        // Unknown code should fail
        let err = registry.resolve_symbol("civil:vor", &style);
        assert_eq!(err.unwrap_err(), SymbologyError::ProviderNotFound);
    }
}
