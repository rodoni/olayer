use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayDimension {
    TwoD,
    TwoPointFiveD,
    ThreeD,
}
