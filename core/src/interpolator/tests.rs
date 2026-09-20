use super::*;
use crate::geodesy::LatLon;
use std::f64::consts::PI;

#[test]
fn test_state_validation() {
    let valid_state = TargetState {
        id: "TGT1".to_string(),
        last_position: LatLon::new(0.0, 0.0, 100.0),
        speed_mps: 100.0,
        track_heading_rad: PI / 2.0,
        vertical_rate_mps: 0.0,
        last_ping_time: 0.0,
    };
    assert!(valid_state.validate().is_ok());

    let invalid_speed = TargetState {
        speed_mps: -1.0,
        ..valid_state.clone()
    };
    assert!(matches!(
        invalid_speed.validate(),
        Err(InterpolatorError::InvalidState(_))
    ));

    let invalid_heading_neg = TargetState {
        track_heading_rad: -0.1,
        ..valid_state.clone()
    };
    assert!(matches!(
        invalid_heading_neg.validate(),
        Err(InterpolatorError::InvalidState(_))
    ));

    let invalid_heading_too_large = TargetState {
        track_heading_rad: 2.0 * PI + 0.01,
        ..valid_state.clone()
    };
    assert!(matches!(
        invalid_heading_too_large.validate(),
        Err(InterpolatorError::InvalidState(_))
    ));

    for invalid_state in [
        TargetState {
            last_position: LatLon::new(f64::NAN, 0.0, 0.0),
            ..valid_state.clone()
        },
        TargetState {
            speed_mps: f64::NAN,
            ..valid_state.clone()
        },
        TargetState {
            speed_mps: f64::INFINITY,
            ..valid_state.clone()
        },
        TargetState {
            vertical_rate_mps: f64::NEG_INFINITY,
            ..valid_state.clone()
        },
    ] {
        assert!(matches!(
            invalid_state.validate(),
            Err(InterpolatorError::InvalidState(_))
        ));
    }
}

#[test]
fn test_heading_boundary_2pi_rejected() {
    let state = TargetState {
        id: "TGT2".to_string(),
        last_position: LatLon::new(0.0, 0.0, 100.0),
        speed_mps: 0.0,
        track_heading_rad: 2.0 * PI,
        vertical_rate_mps: 0.0,
        last_ping_time: 0.0,
    };
    assert!(state.validate().is_err());
}

#[test]
fn test_state_rejects_empty_id_and_non_finite_timestamp() {
    let state = TargetState {
        id: String::new(),
        last_position: LatLon::new(0.0, 0.0, 100.0),
        speed_mps: 0.0,
        track_heading_rad: 0.0,
        vertical_rate_mps: 0.0,
        last_ping_time: f64::NAN,
    };
    assert!(state.validate().is_err());
}

#[test]
fn test_engine_crud() {
    let mut engine = InterpolationEngine::new();
    let state = TargetState {
        id: "TGT1".to_string(),
        last_position: LatLon::new(0.0, 0.0, 100.0),
        speed_mps: 100.0,
        track_heading_rad: PI / 2.0,
        vertical_rate_mps: 0.0,
        last_ping_time: 0.0,
    };

    assert!(engine.update_target(state).is_ok());
    assert!(engine.remove_target("TGT1"));
    assert!(!engine.remove_target("TGT1"));
}

#[test]
fn test_horizontal_translation() {
    let mut engine = InterpolationEngine::new();
    let start_pos = LatLon::from_degrees(-23.5505, -46.6333, 1000.0); // São Paulo
    let heading = 90.0_f64.to_radians(); // East
    let speed = 250.0; // m/s

    let state = TargetState {
        id: "ALVO1".to_string(),
        last_position: start_pos,
        speed_mps: speed,
        track_heading_rad: heading,
        vertical_rate_mps: 0.0,
        last_ping_time: 100.0,
    };

    engine.update_target(state).unwrap();

    // Interpolate at t = 110s (dt = 10s, distance = 2500m)
    let results = engine.interpolate_all(110.0).unwrap();
    assert_eq!(results.len(), 1);
    let target = &results[0];
    assert_eq!(target.id, "ALVO1");

    // Latitude should remain extremely close to the start since we headed due east
    assert!((target.position.lat - start_pos.lat).abs() < 1e-5);
    // Longitude should have increased (moved East)
    assert!(target.position.lon > start_pos.lon);
    assert_eq!(target.heading_rad, heading);
}

#[test]
fn test_vertical_rate_translation() {
    let mut engine = InterpolationEngine::new();
    let start_pos = LatLon::from_degrees(0.0, 0.0, 1000.0);

    let state = TargetState {
        id: "CLIMBER".to_string(),
        last_position: start_pos,
        speed_mps: 0.0,
        track_heading_rad: 0.0,
        vertical_rate_mps: 15.0, // climbing at 15 m/s
        last_ping_time: 0.0,
    };

    engine.update_target(state).unwrap();

    let results = engine.interpolate_all(10.0).unwrap();
    assert_eq!(results.len(), 1);
    // 1000 + 15 * 10 = 1150 meters
    assert_eq!(results[0].position.height, 1150.0);
}

#[test]
fn test_interpolation_handles_dateline_and_polar_positions() {
    let mut engine = InterpolationEngine::new();
    for (id, position) in [
        ("dateline", LatLon::from_degrees(0.0, 179.9, 100.0)),
        ("polar", LatLon::from_degrees(89.0, 0.0, 100.0)),
    ] {
        engine
            .update_target(TargetState {
                id: id.to_string(),
                last_position: position,
                speed_mps: 100.0,
                track_heading_rad: PI / 2.0,
                vertical_rate_mps: 0.0,
                last_ping_time: 0.0,
            })
            .unwrap();
    }

    let batch = engine.interpolate_all_with_status(10.0).unwrap();
    assert_eq!(batch.targets.len(), 2);
    assert!(batch
        .targets
        .iter()
        .all(|target| target.position.lat.is_finite()
            && target.position.lon.is_finite()
            && target.position.height.is_finite()));
}

#[test]
fn test_stale_targets_exclusion() {
    let mut engine = InterpolationEngine::with_stale_threshold(15.0).unwrap();
    let state = TargetState {
        id: "TGT1".to_string(),
        last_position: LatLon::new(0.0, 0.0, 100.0),
        speed_mps: 10.0,
        track_heading_rad: 0.0,
        vertical_rate_mps: 0.0,
        last_ping_time: 100.0,
    };

    engine.update_target(state).unwrap();

    // At t = 110.0 (dt = 10.0s <= 15.0s), target should be present
    let res_active = engine.interpolate_all(110.0).unwrap();
    assert_eq!(res_active.len(), 1);

    // At t = 120.0 (dt = 20.0s > 15.0s), target is stale and should be excluded
    let res_stale = engine.interpolate_all(120.0).unwrap();
    assert!(res_stale.is_empty());
}

#[test]
fn test_invalid_stale_thresholds_rejected() {
    for threshold in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            InterpolationEngine::with_stale_threshold(threshold),
            Err(InterpolatorError::InvalidState(_))
        ));
    }
    assert!(InterpolationEngine::with_stale_threshold(0.0).is_ok());
}

#[test]
fn test_negative_time_delta_skipped_not_aborted() {
    let mut engine = InterpolationEngine::with_stale_threshold(60.0).unwrap();
    let bad_state = TargetState {
        id: "BAD".to_string(),
        last_position: LatLon::new(0.0, 0.0, 100.0),
        speed_mps: 10.0,
        track_heading_rad: 0.0,
        vertical_rate_mps: 0.0,
        last_ping_time: 100.0,
    };
    let good_state = TargetState {
        id: "GOOD".to_string(),
        last_position: LatLon::new(0.0, 0.0, 200.0),
        speed_mps: 10.0,
        track_heading_rad: 0.0,
        vertical_rate_mps: 0.0,
        last_ping_time: 50.0,
    };

    engine.update_target(bad_state).unwrap();
    engine.update_target(good_state).unwrap();

    // Querying at t = 99.0s:
    // BAD has dt = -1.0s (skipped)
    // GOOD has dt = 49.0s (active, within 60s threshold)
    let results = engine.interpolate_all(99.0).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "GOOD");
}

#[test]
fn test_status_reports_stale_and_clock_skewed_targets() {
    let mut engine = InterpolationEngine::with_stale_threshold(15.0).unwrap();
    engine
        .update_target(TargetState {
            id: "stale".to_string(),
            last_position: LatLon::new(0.0, 0.0, 0.0),
            speed_mps: 0.0,
            track_heading_rad: 0.0,
            vertical_rate_mps: 0.0,
            last_ping_time: 100.0,
        })
        .unwrap();
    engine
        .update_target(TargetState {
            id: "future".to_string(),
            last_position: LatLon::new(0.0, 0.0, 0.0),
            speed_mps: 0.0,
            track_heading_rad: 0.0,
            vertical_rate_mps: 0.0,
            last_ping_time: 130.0,
        })
        .unwrap();

    let batch = engine.interpolate_all_with_status(120.0).unwrap();
    assert!(batch.targets.is_empty());
    assert_eq!(batch.skipped.len(), 2);
    assert!(batch
        .skipped
        .iter()
        .any(|target| target.quality == PredictionQuality::Stale));
    assert!(batch
        .skipped
        .iter()
        .any(|target| target.quality == PredictionQuality::ClockSkewed));
}

#[test]
fn test_status_reports_unavailable_for_non_finite_time() {
    let mut engine = InterpolationEngine::new();
    engine
        .update_target(TargetState {
            id: "target".to_string(),
            last_position: LatLon::new(0.0, 0.0, 0.0),
            speed_mps: 0.0,
            track_heading_rad: 0.0,
            vertical_rate_mps: 0.0,
            last_ping_time: 10.0,
        })
        .unwrap();

    let batch = engine.interpolate_all_with_status(f64::NAN).unwrap();
    assert_eq!(batch.skipped[0].quality, PredictionQuality::Unavailable);
}

#[test]
fn test_multiple_targets_interpolation() {
    let mut engine = InterpolationEngine::new();

    let state_a = TargetState {
        id: "A".to_string(),
        last_position: LatLon::from_degrees(0.0, 0.0, 100.0),
        speed_mps: 100.0,
        track_heading_rad: PI / 2.0,
        vertical_rate_mps: 0.0,
        last_ping_time: 0.0,
    };
    let state_b = TargetState {
        id: "B".to_string(),
        last_position: LatLon::from_degrees(0.0, 0.0, 200.0),
        speed_mps: 0.0,
        track_heading_rad: 0.0,
        vertical_rate_mps: 10.0,
        last_ping_time: 0.0,
    };

    engine.update_target(state_a).unwrap();
    engine.update_target(state_b).unwrap();

    let results = engine.interpolate_all(10.0).unwrap();
    assert_eq!(results.len(), 2);

    // Verify both targets are present
    let ids: Vec<_> = results.iter().map(|r| r.id.as_str()).collect();
    assert!(ids.contains(&"A"));
    assert!(ids.contains(&"B"));

    let a = results.iter().find(|r| r.id == "A").unwrap();
    let b = results.iter().find(|r| r.id == "B").unwrap();

    assert!(a.position.lon > 0.0); // A moved east
    assert_eq!(b.position.height, 300.0); // B climbed 10 m/s * 10 s
}

#[test]
fn test_default_impl() {
    let engine: InterpolationEngine = Default::default();
    assert!(engine.interpolate_all(0.0).unwrap().is_empty());
}

#[test]
fn test_interpolator_error_display() {
    assert_eq!(
        InterpolatorError::InvalidState("bad speed".to_string()).to_string(),
        "Invalid target state: bad speed"
    );
    assert_eq!(
        InterpolatorError::GeodesyFailure(crate::geodesy::GeodesyError::NonFiniteLatitude)
            .to_string(),
        "Geodesy calculation failed: latitude is not finite"
    );
}
