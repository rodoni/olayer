mod errors;
pub mod parser;
mod styles;
#[cfg(test)]
mod tests;

pub use errors::SldError;
pub use parser::parse;
pub use styles::{FillStyle, PointStyle, RuleStyle, StrokeStyle, StyleRegistry, TextStyle};
