use crate::declutter::engine::DeclutterEngine;
use crate::declutter::spatial_grid::{line_segments_intersect, SpatialHashGrid};
use crate::declutter::types::{DeclutterConfig, LabelTarget, OctantDirection, Rect2D};

#[test]
fn test_rect2d_and_spatial_grid() {
    let r1 = Rect2D::new(0.0, 0.0, 10.0, 10.0);
    let r2 = Rect2D::new(5.0, 5.0, 10.0, 10.0);
    let r3 = Rect2D::new(40.0, 40.0, 10.0, 10.0);

    assert!(r1.intersects(&r2));
    assert!(!r1.intersects(&r3));
    assert_eq!(r1.intersection_area(&r2), 25.0);

    let mut grid = SpatialHashGrid::new(32.0);
    grid.insert(0, &r1);
    grid.insert(1, &r2);
    grid.insert(2, &r3);

    let candidates = grid.query_candidates(&r1);
    assert!(candidates.contains(&0));
    assert!(candidates.contains(&1));
    assert!(!candidates.contains(&2));
}

#[test]
fn test_line_segments_intersect() {
    // Intersecting X
    let p1 = [0.0, 0.0];
    let p2 = [10.0, 10.0];
    let p3 = [0.0, 10.0];
    let p4 = [10.0, 0.0];
    assert!(line_segments_intersect(p1, p2, p3, p4));

    // Parallel lines
    let p5 = [0.0, 5.0];
    let p6 = [10.0, 15.0];
    assert!(!line_segments_intersect(p1, p2, p5, p6));
}

#[test]
fn test_single_target_prefers_northeast() {
    let engine = DeclutterEngine::with_default_config();
    let targets = vec![LabelTarget {
        id: "AFR101".to_string(),
        x: 100.0,
        y: 100.0,
        heading_rad: None,
        width: 60.0,
        height: 25.0,
        priority: 0,
    }];

    let solved = engine.solve(&targets);
    assert_eq!(solved.len(), 1);
    assert_eq!(solved[0].octant, OctantDirection::NorthEast);
    assert!(solved[0].rect.x > 100.0);
    assert!(solved[0].rect.y < 100.0);
}

#[test]
fn test_heading_avoidance() {
    let engine = DeclutterEngine::with_default_config();
    // Aircraft heading North-East (45 deg = PI/4 rad)
    let targets = vec![LabelTarget {
        id: "BAW202".to_string(),
        x: 200.0,
        y: 200.0,
        heading_rad: Some(std::f32::consts::FRAC_PI_4),
        width: 60.0,
        height: 25.0,
        priority: 0,
    }];

    let solved = engine.solve(&targets);
    assert_eq!(solved.len(), 1);
    // Should NOT be NorthEast because heading is NorthEast
    assert_ne!(solved[0].octant, OctantDirection::NorthEast);
}

#[test]
fn test_cluster_deconfliction_no_overlap() {
    let engine = DeclutterEngine::new(DeclutterConfig {
        leader_length_px: 25.0,
        safety_margin_px: 2.0,
        weight_overlap: 10000.0,
        weight_heading: 10.0,
        weight_leader_crossing: 100.0,
        weight_preference: 1.0,
        max_iterations: 5,
    });

    // 4 aircraft tightly clustered within 15 pixels of each other
    let targets = vec![
        LabelTarget {
            id: "T1".to_string(),
            x: 200.0,
            y: 200.0,
            heading_rad: None,
            width: 50.0,
            height: 20.0,
            priority: 0,
        },
        LabelTarget {
            id: "T2".to_string(),
            x: 205.0,
            y: 205.0,
            heading_rad: None,
            width: 50.0,
            height: 20.0,
            priority: 1,
        },
        LabelTarget {
            id: "T3".to_string(),
            x: 195.0,
            y: 205.0,
            heading_rad: None,
            width: 50.0,
            height: 20.0,
            priority: 2,
        },
        LabelTarget {
            id: "T4".to_string(),
            x: 200.0,
            y: 195.0,
            heading_rad: None,
            width: 50.0,
            height: 20.0,
            priority: 3,
        },
    ];

    let solved = engine.solve(&targets);
    assert_eq!(solved.len(), 4);

    // Verify all 4 labels have distinct non-overlapping bounding boxes
    for i in 0..4 {
        for j in (i + 1)..4 {
            let r1 = &solved[i].rect;
            let r2 = &solved[j].rect;
            assert!(
                !r1.intersects(r2),
                "Labels {} and {} overlap! {:?} vs {:?}",
                solved[i].id,
                solved[j].id,
                r1,
                r2
            );
        }
    }
}
