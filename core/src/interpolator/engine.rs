use std::collections::HashMap;
use crate::geodesy::{Ellipsoid, GeodeticSolver, HaversineSolver, VincentySolver};
use crate::interpolator::errors::InterpolatorError;
use crate::interpolator::state::{InterpolatedTarget, TargetState};

pub struct InterpolationEngine {
    targets: HashMap<String, TargetState>,
    stale_threshold: f64,
    // Cached solver/ellipsoid instances to avoid reconstructing them every frame.
    vincenty: VincentySolver,
    haversine: HaversineSolver,
    ellipsoid: Ellipsoid,
}

impl InterpolationEngine {
    /// Creates a new [`InterpolationEngine`] with a default stale threshold of 30.0 seconds.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            targets: HashMap::new(),
            stale_threshold: 30.0,
            vincenty: VincentySolver,
            haversine: HaversineSolver,
            ellipsoid: Ellipsoid::wgs84(),
        }
    }

    /// Creates a new [`InterpolationEngine`] with a custom stale threshold in seconds.
    #[inline]
    #[must_use]
    pub fn with_stale_threshold(stale_threshold: f64) -> Self {
        Self {
            targets: HashMap::new(),
            stale_threshold,
            vincenty: VincentySolver,
            haversine: HaversineSolver,
            ellipsoid: Ellipsoid::wgs84(),
        }
    }

    /// Inserts or updates a target state. Validates the state before insertion.
    ///
    /// # Errors
    ///
    /// Returns `InterpolatorError::InvalidState` if the target state contains
    /// invalid physical parameters (e.g. negative speed, out-of-range heading).
    #[inline]
    pub fn update_target(&mut self, state: TargetState) -> Result<(), InterpolatorError> {
        state.validate()?;
        self.targets.insert(state.id.clone(), state);
        Ok(())
    }

    /// Removes a target by its identifier. Returns `true` if the target was present.
    #[inline]
    pub fn remove_target(&mut self, id: &str) -> bool {
        self.targets.remove(id).is_some()
    }

    /// Interpolates the positions of all active targets to the given `current_time`.
    ///
    /// Stale targets (whose time difference exceeds `stale_threshold`) are silently
    /// skipped. Targets with a negative `dt` are also skipped rather than aborting the
    /// entire batch, ensuring that a single rogue sensor does not freeze the display.
    ///
    /// # Errors
    ///
    /// Returns `Err` only if a geodetic calculation fails unexpectedly (e.g. numeric
    /// instability in the Vincenty solver that also fails in the Haversine fallback).
    #[inline]
    pub fn interpolate_all(&self, current_time: f64) -> Result<Vec<InterpolatedTarget>, InterpolatorError> {
        let mut results = Vec::with_capacity(self.targets.len());

        for (id, state) in &self.targets {
            let dt = current_time - state.last_ping_time;

            // Skip targets with retrograde time (clock skew) — do not abort the batch
            if dt < 0.0 {
                continue;
            }

            // Exclude stale targets
            if dt > self.stale_threshold {
                continue;
            }

            // 1. Horizontal translation
            let dist = state.speed_mps * dt;
            let next_pos = if dist > 0.0 {
                // Try VincentySolver first, fallback to HaversineSolver if it fails
                match self.vincenty.direct(&state.last_position, state.track_heading_rad, dist, &self.ellipsoid) {
                    Ok(pos) => pos,
                    Err(_) => {
                        self.haversine.direct(&state.last_position, state.track_heading_rad, dist, &self.ellipsoid)?
                    }
                }
            } else {
                state.last_position
            };

            // 2. Vertical rate translation (apply vertical speed to altitude/height)
            let mut final_pos = next_pos;
            final_pos.height = state.last_position.height + state.vertical_rate_mps * dt;

            results.push(InterpolatedTarget {
                id: id.clone(),
                position: final_pos,
                heading_rad: state.track_heading_rad,
            });
        }

        Ok(results)
    }
}

impl Default for InterpolationEngine {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
