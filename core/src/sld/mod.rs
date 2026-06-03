mod errors;
mod styles;
pub mod parser;
#[cfg(test)]
mod tests;

pub use errors::SldError;
pub use styles::{StyleRegistry, RuleStyle, StrokeStyle, FillStyle, TextStyle, PointStyle};
pub use parser::parse;
