use std::collections::HashMap;

/// Trait for all visualization layers in the native Olayer framework.
///
/// Layers are rendered in strict back-to-front order:
///
/// ```text
///        [ Top ]
///    ┌───────────────┐
///    │ egui HUD / UI │  <-- Layer 4: Interactive Controls & Panels
///    └───────────────┘
///    ┌───────────────┐
///    │ Radar Targets │  <-- Layer 3: Aircraft & Velocity Vectors
///    └───────────────┘
///    ┌───────────────┐
///    │ Geodetic Grid │  <-- Layer 2: Longitude & Latitude Lines
///    └───────────────┘
///    ┌───────────────┐
///    │ Terrain Base  │  <-- Layer 1: Elevation / DTED Background
///    └───────────────┘
///    ┌───────────────┐
///    │ Background    │  <-- Layer 0: ATC Radar Screen Color
///    └───────────────┘
///       [ Bottom ]
/// ```
///
/// **Static layers** (grid, terrain, background) are only regenerated when the
/// camera changes pan, zoom, or projection.  **Dynamic layers** (targets, HUD)
/// are redrawn every frame.
pub trait Layer {
    /// Unique identifier for this layer.
    fn id(&self) -> &str;
    /// Whether this layer is currently visible.
    fn is_visible(&self) -> bool;
    /// Toggle visibility.
    fn set_visible(&mut self, visible: bool);
    /// Whether this layer is static (rarely changes) or dynamic (updated every frame).
    fn is_static(&self) -> bool;
}

/// Orchestrates the stack of active layers, handling visibility, compositing order,
/// and render cycle segregation (static vs dynamic).
///
/// Also provides fast boolean toggles for the hardcoded layers used by the demo app.
pub struct NativeLayerManager {
    layers: Vec<Box<dyn Layer>>,
    index: HashMap<String, usize>,
    /// Convenience toggle for the geodetic grid layer (demo use).
    pub show_grid: bool,
    /// Convenience toggle for the radar targets layer (demo use).
    pub show_targets: bool,
    /// Convenience toggle for the HUD/UI layer (demo use).
    pub show_hud: bool,
    /// Convenience toggle for the terrain base layer (demo use).
    pub show_terrain: bool,
}

impl Default for NativeLayerManager {
    fn default() -> Self {
        Self {
            layers: Vec::new(),
            index: HashMap::new(),
            show_grid: true,
            show_targets: true,
            show_hud: true,
            show_terrain: true,
        }
    }
}

impl NativeLayerManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a new layer to the top of the stack.
    ///
    /// Returns `Err` if a layer with the same ID already exists.
    pub fn add_layer(&mut self, layer: Box<dyn Layer>) -> Result<(), String> {
        let id = layer.id().to_string();
        if self.index.contains_key(&id) {
            return Err(format!("Layer with id '{}' already exists.", id));
        }
        self.index.insert(id, self.layers.len());
        self.layers.push(layer);
        Ok(())
    }

    /// Removes a layer from the stack by its identifier.
    ///
    /// Returns `true` if the layer was found and removed.
    pub fn remove_layer(&mut self, id: &str) -> bool {
        if let Some(idx) = self.index.remove(id) {
            self.layers.remove(idx);
            // Rebuild index
            for (i, layer) in self.layers.iter().enumerate() {
                self.index.insert(layer.id().to_string(), i);
            }
            true
        } else {
            false
        }
    }

    /// Reorders a layer to a specific index in the stack.
    ///
    /// Returns `Err` if the layer is not found or the target index is out of bounds.
    pub fn reorder_layer(&mut self, id: &str, new_index: usize) -> Result<(), String> {
        let current_idx = *self.index.get(id).ok_or_else(|| format!("Layer '{}' not found.", id))?;
        if new_index >= self.layers.len() {
            return Err(format!(
                "Invalid target index: {} (max {})",
                new_index,
                self.layers.len().saturating_sub(1)
            ));
        }

        let layer = self.layers.remove(current_idx);
        self.layers.insert(new_index, layer);

        // Rebuild index
        for (i, layer) in self.layers.iter().enumerate() {
            self.index.insert(layer.id().to_string(), i);
        }
        Ok(())
    }

    /// Returns a slice of all layers in render order (back-to-front).
    pub fn get_layers(&self) -> &[Box<dyn Layer>] {
        &self.layers
    }

    /// Returns all visible static layers.
    pub fn visible_static_layers(&self) -> Vec<&dyn Layer> {
        self.layers.iter().filter(|l| l.is_visible() && l.is_static()).map(|b| b.as_ref()).collect()
    }

    /// Returns all visible dynamic layers.
    pub fn visible_dynamic_layers(&self) -> Vec<&dyn Layer> {
        self.layers.iter().filter(|l| l.is_visible() && !l.is_static()).map(|b| b.as_ref()).collect()
    }

    /// Toggles visibility for a specific layer.
    ///
    /// Returns `Err` if the layer is not found.
    pub fn set_layer_visibility(&mut self, id: &str, visible: bool) -> Result<(), String> {
        let idx = self.index.get(id).copied().ok_or_else(|| format!("Layer '{}' not found.", id))?;
        self.layers[idx].set_visible(visible);
        Ok(())
    }

    /// Toggles all layers on or off.
    pub fn set_all_visibility(&mut self, visible: bool) {
        for layer in &mut self.layers {
            layer.set_visible(visible);
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    struct MockLayer {
        id: String,
        visible: bool,
        static_layer: bool,
    }

    impl Layer for MockLayer {
        fn id(&self) -> &str {
            &self.id
        }
        fn is_visible(&self) -> bool {
            self.visible
        }
        fn set_visible(&mut self, visible: bool) {
            self.visible = visible;
        }
        fn is_static(&self) -> bool {
            self.static_layer
        }
    }

    #[test]
    fn test_add_and_remove_layer() {
        let mut mgr = NativeLayerManager::new();
        mgr.add_layer(Box::new(MockLayer {
            id: "A".to_string(),
            visible: true,
            static_layer: true,
        }))
        .unwrap();
        mgr.add_layer(Box::new(MockLayer {
            id: "B".to_string(),
            visible: true,
            static_layer: false,
        }))
        .unwrap();

        assert_eq!(mgr.get_layers().len(), 2);
        assert!(mgr.remove_layer("A"));
        assert_eq!(mgr.get_layers().len(), 1);
        assert!(!mgr.remove_layer("C"));
    }

    #[test]
    fn test_duplicate_layer_rejected() {
        let mut mgr = NativeLayerManager::new();
        mgr.add_layer(Box::new(MockLayer {
            id: "A".to_string(),
            visible: true,
            static_layer: true,
        }))
        .unwrap();
        let result = mgr.add_layer(Box::new(MockLayer {
            id: "A".to_string(),
            visible: true,
            static_layer: true,
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_reorder_layer() {
        let mut mgr = NativeLayerManager::new();
        mgr.add_layer(Box::new(MockLayer {
            id: "A".to_string(),
            visible: true,
            static_layer: true,
        }))
        .unwrap();
        mgr.add_layer(Box::new(MockLayer {
            id: "B".to_string(),
            visible: true,
            static_layer: false,
        }))
        .unwrap();
        mgr.add_layer(Box::new(MockLayer {
            id: "C".to_string(),
            visible: true,
            static_layer: true,
        }))
        .unwrap();

        mgr.reorder_layer("B", 0).unwrap();
        assert_eq!(mgr.get_layers()[0].id(), "B");
        assert_eq!(mgr.get_layers()[1].id(), "A");
        assert_eq!(mgr.get_layers()[2].id(), "C");
    }

    #[test]
    fn test_reorder_invalid_index() {
        let mut mgr = NativeLayerManager::new();
        mgr.add_layer(Box::new(MockLayer {
            id: "A".to_string(),
            visible: true,
            static_layer: true,
        }))
        .unwrap();
        assert!(mgr.reorder_layer("A", 5).is_err());
        assert!(mgr.reorder_layer("Z", 0).is_err());
    }

    #[test]
    fn test_visibility_and_static_dynamic_segregation() {
        let mut mgr = NativeLayerManager::new();
        mgr.add_layer(Box::new(MockLayer {
            id: "Static1".to_string(),
            visible: true,
            static_layer: true,
        }))
        .unwrap();
        mgr.add_layer(Box::new(MockLayer {
            id: "Dynamic1".to_string(),
            visible: false,
            static_layer: false,
        }))
        .unwrap();
        mgr.add_layer(Box::new(MockLayer {
            id: "Static2".to_string(),
            visible: true,
            static_layer: true,
        }))
        .unwrap();

        let static_layers = mgr.visible_static_layers();
        assert_eq!(static_layers.len(), 2);
        assert_eq!(static_layers[0].id(), "Static1");

        assert_eq!(mgr.visible_dynamic_layers().len(), 0);

        mgr.set_layer_visibility("Dynamic1", true).unwrap();
        assert_eq!(mgr.visible_dynamic_layers().len(), 1);
        assert_eq!(mgr.visible_dynamic_layers()[0].id(), "Dynamic1");
    }

    #[test]
    fn test_set_all_visibility() {
        let mut mgr = NativeLayerManager::new();
        mgr.add_layer(Box::new(MockLayer {
            id: "A".to_string(),
            visible: true,
            static_layer: true,
        }))
        .unwrap();
        mgr.add_layer(Box::new(MockLayer {
            id: "B".to_string(),
            visible: true,
            static_layer: false,
        }))
        .unwrap();

        mgr.set_all_visibility(false);
        assert!(!mgr.get_layers()[0].is_visible());
        assert!(!mgr.get_layers()[1].is_visible());
    }
}
