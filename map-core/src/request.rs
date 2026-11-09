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
    /// Joins the endpoint and appends RFC 3986 encoded query parameters in order.
    ///
    /// Existing query text is preserved verbatim, before the new parameters.
    /// A fragment in `path` overrides a fragment in `base`; fragments are always
    /// emitted last. This joins components, not relative URLs, and does not
    /// validate schemes or normalize existing query text.
    pub fn url(&self, params: &[QueryParameter]) -> String {
        let (base, base_query, base_fragment) = components(&self.base);
        let (path, path_query, path_fragment) = components(&self.path);
        let mut out = base.trim_end_matches('/').to_owned();
        if !path.is_empty() {
            if !path.starts_with('/') {
                out.push('/');
            }
            out.push_str(path);
        }
        let mut separator = '?';
        for query in [base_query, path_query].into_iter().flatten() {
            if separator == '?' || !out.ends_with(['?', '&']) {
                out.push(separator);
            }
            out.push_str(query);
            separator = '&';
        }
        for parameter in params {
            if separator == '?' || !out.ends_with(['?', '&']) {
                out.push(separator);
            }
            QueryParameter::encode_into(&parameter.key, &mut out);
            out.push('=');
            QueryParameter::encode_into(&parameter.value, &mut out);
            separator = '&';
        }
        if let Some(fragment) = path_fragment.or(base_fragment) {
            out.push('#');
            out.push_str(fragment);
        }
        out
    }
}

fn components(value: &str) -> (&str, Option<&str>, Option<&str>) {
    let (value, fragment) = value
        .split_once('#')
        .map_or((value, None), |(value, fragment)| (value, Some(fragment)));
    let (path, query) = value
        .split_once('?')
        .map_or((value, None), |(path, query)| (path, Some(query)));
    (path, query, fragment)
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
    fn encode_into(value: &str, out: &mut String) {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        for byte in value.bytes() {
            if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
                out.push(char::from(byte));
            } else {
                out.push('%');
                out.push(char::from(HEX[usize::from(byte >> 4)]));
                out.push(char::from(HEX[usize::from(byte & 15)]));
            }
        }
    }
}
