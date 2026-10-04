use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UrlParts {
    pub base: String,
    pub path: String,
}
impl UrlParts {
    pub fn new(base: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            base: base.into(),
            path: path.into(),
        }
    }
    pub fn url(&self, params: &[QueryParameter]) -> String {
        let mut out = format!(
            "{}{}",
            self.base.trim_end_matches('/'),
            if self.path.is_empty() {
                String::new()
            } else if self.path.starts_with('/') {
                self.path.clone()
            } else {
                format!("/{}", self.path)
            }
        );
        if !params.is_empty() {
            out.push('?');
            out.push_str(
                &params
                    .iter()
                    .map(QueryParameter::to_query)
                    .collect::<Vec<_>>()
                    .join("&"),
            );
        }
        out
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryParameter {
    pub key: String,
    pub value: String,
}
impl QueryParameter {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
    fn encode(value: &str) -> String {
        value
            .bytes()
            .flat_map(|b| {
                if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                    vec![b as char]
                } else {
                    format!("%{b:02X}").chars().collect()
                }
            })
            .collect()
    }
    fn to_query(&self) -> String {
        format!("{}={}", Self::encode(&self.key), Self::encode(&self.value))
    }
}
