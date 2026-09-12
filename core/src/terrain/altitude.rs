use serde::{Deserialize, Serialize};

/// Defines how an object's input height is interpreted against terrain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AltitudeMode {
    /// The input height is already an absolute geodetic height.
    Absolute,
    /// Place the object directly on the sampled ground surface.
    ClampToGround,
    /// Interpret the input as an offset above the sampled ground surface.
    RelativeToGround,
    /// Interpret the input as an offset above the effective rendered mesh.
    RelativeToMesh,
}

/// Defines what to do when no terrain sample is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AltitudeUnknownPolicy {
    Reject,
    UseAbsolute,
    UseZero,
}

/// Resolves an object's final height without applying a projection or vertical exaggeration.
pub fn resolve_altitude(
    input_height: f64,
    ground_height: Option<f64>,
    mesh_height: Option<f64>,
    mode: AltitudeMode,
    unknown_policy: AltitudeUnknownPolicy,
) -> Result<f64, &'static str> {
    if !input_height.is_finite() {
        return Err("input height must be finite");
    }

    let required_height = match mode {
        AltitudeMode::Absolute => return Ok(input_height),
        AltitudeMode::RelativeToMesh => mesh_height.or(ground_height),
        AltitudeMode::ClampToGround | AltitudeMode::RelativeToGround => ground_height,
    };

    let base = match required_height {
        Some(value) if value.is_finite() => value,
        Some(_) => return Err("terrain height must be finite"),
        None => match unknown_policy {
            AltitudeUnknownPolicy::Reject => return Err("terrain elevation is unavailable"),
            AltitudeUnknownPolicy::UseAbsolute => return Ok(input_height),
            AltitudeUnknownPolicy::UseZero => 0.0,
        },
    };

    Ok(match mode {
        AltitudeMode::ClampToGround => base,
        AltitudeMode::RelativeToGround | AltitudeMode::RelativeToMesh => base + input_height,
        AltitudeMode::Absolute => input_height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_supported_modes() {
        assert_eq!(resolve_altitude(120.0, Some(800.0), Some(850.0), AltitudeMode::Absolute, AltitudeUnknownPolicy::Reject).unwrap(), 120.0);
        assert_eq!(resolve_altitude(120.0, Some(800.0), Some(850.0), AltitudeMode::ClampToGround, AltitudeUnknownPolicy::Reject).unwrap(), 800.0);
        assert_eq!(resolve_altitude(120.0, Some(800.0), Some(850.0), AltitudeMode::RelativeToGround, AltitudeUnknownPolicy::Reject).unwrap(), 920.0);
        assert_eq!(resolve_altitude(120.0, Some(800.0), Some(850.0), AltitudeMode::RelativeToMesh, AltitudeUnknownPolicy::Reject).unwrap(), 970.0);
    }

    #[test]
    fn applies_unknown_terrain_policy() {
        assert!(resolve_altitude(120.0, None, None, AltitudeMode::RelativeToGround, AltitudeUnknownPolicy::Reject).is_err());
        assert_eq!(resolve_altitude(120.0, None, None, AltitudeMode::RelativeToGround, AltitudeUnknownPolicy::UseAbsolute).unwrap(), 120.0);
        assert_eq!(resolve_altitude(120.0, None, None, AltitudeMode::RelativeToGround, AltitudeUnknownPolicy::UseZero).unwrap(), 120.0);
    }
}
