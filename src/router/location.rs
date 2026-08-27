use std::{collections::BTreeMap, fmt};

/// A complete navigation target. History stores `Location`, rather than a matched
/// route identifier, so dynamic parameters and query state survive back/replace.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Location {
    path: String,
    query: Option<String>,
    fragment: Option<String>,
}

impl Default for Location {
    fn default() -> Self {
        Self::new("/")
    }
}

impl Location {
    pub fn new(target: impl AsRef<str>) -> Self {
        let target = target.as_ref();
        let (without_fragment, fragment) = split_once_non_empty(target, '#');
        let (path, query) = split_once_non_empty(without_fragment, '?');
        Self {
            path: normalize_absolute_path(path),
            query: query.map(str::to_owned),
            fragment: fragment.map(str::to_owned),
        }
    }

    /// Percent-encodes a single dynamic path segment. Slashes are encoded so a
    /// value can never accidentally change the declared route hierarchy.
    pub fn encode_path_segment(value: &str) -> String {
        let mut encoded = String::with_capacity(value.len());
        for byte in value.as_bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
                encoded.push(*byte as char);
            } else {
                encoded.push('%');
                encoded.push(HEX[(byte >> 4) as usize] as char);
                encoded.push(HEX[(byte & 0x0f) as usize] as char);
            }
        }
        encoded
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }

    pub fn fragment(&self) -> Option<&str> {
        self.fragment.as_deref()
    }

    pub fn href(&self) -> String {
        let mut href = self.path.clone();
        if let Some(query) = &self.query {
            href.push('?');
            href.push_str(query);
        }
        if let Some(fragment) = &self.fragment {
            href.push('#');
            href.push_str(fragment);
        }
        href
    }

    pub fn with_query(mut self, query: impl Into<String>) -> Self {
        let query = query.into();
        self.query = (!query.is_empty()).then_some(query);
        self
    }

    pub fn with_fragment(mut self, fragment: impl Into<String>) -> Self {
        let fragment = fragment.into();
        self.fragment = (!fragment.is_empty()).then_some(fragment);
        self
    }

    /// Resolves an absolute or URL-style relative target against this location.
    /// Route-relative navigation can use `RouteMatches::resolve_from` so a route
    /// consuming multiple path segments still resolves `..` to its parent route.
    pub fn resolve(&self, target: impl AsRef<str>) -> Self {
        let target = target.as_ref();
        if target.starts_with('/') {
            return Self::new(target);
        }
        if target.starts_with('?') {
            return Self::new(format!("{}{}", self.path, target));
        }
        if target.starts_with('#') {
            let query = self
                .query
                .as_ref()
                .map(|query| format!("?{query}"))
                .unwrap_or_default();
            return Self::new(format!("{}{}{}", self.path, query, target));
        }

        let base = self
            .path
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or_default();
        Self::new(resolve_path(base, target))
    }

    pub(crate) fn segments(&self) -> Vec<&str> {
        path_segments(&self.path)
    }
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

impl From<&str> for Location {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for Location {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for Location {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.href())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathParams(BTreeMap<String, String>);

impl PathParams {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn extend(&mut self, values: impl IntoIterator<Item = (String, String)>) -> bool {
        let values = values.into_iter().collect::<Vec<_>>();
        if values.iter().any(|(name, _)| self.0.contains_key(name)) {
            return false;
        }
        for (name, value) in values {
            self.0.insert(name, value);
        }
        true
    }
}

pub(crate) fn path_segments(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

pub(crate) fn resolve_path(base: &str, target: &str) -> String {
    let mut segments = path_segments(base)
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let (target_path, suffix) = split_suffix(target);
    for segment in target_path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            segment => segments.push(segment.to_owned()),
        }
    }
    let mut path = format!("/{}", segments.join("/"));
    if path.len() > 1 && target_path.ends_with('/') {
        path.push('/');
    }
    path.push_str(suffix);
    path
}

pub(crate) fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        let high = *bytes.get(index + 1)?;
        let low = *bytes.get(index + 2)?;
        decoded.push((hex_value(high)? << 4) | hex_value(low)?);
        index += 3;
    }
    String::from_utf8(decoded).ok()
}

fn normalize_absolute_path(path: &str) -> String {
    let mut segments = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            segment => segments.push(segment),
        }
    }
    if segments.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", segments.join("/"))
    }
}

fn split_once_non_empty(value: &str, delimiter: char) -> (&str, Option<&str>) {
    value
        .split_once(delimiter)
        .map(|(head, tail)| (head, (!tail.is_empty()).then_some(tail)))
        .unwrap_or((value, None))
}

fn split_suffix(target: &str) -> (&str, &str) {
    let query = target.find('?');
    let fragment = target.find('#');
    let split = match (query, fragment) {
        (Some(query), Some(fragment)) => query.min(fragment),
        (Some(query), None) => query,
        (None, Some(fragment)) => fragment,
        (None, None) => return (target, ""),
    };
    target.split_at(split)
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_keeps_complete_navigation_state() {
        let location = Location::new("community//articles/42/?tab=comments#reply");
        assert_eq!(location.path(), "/community/articles/42");
        assert_eq!(location.query(), Some("tab=comments"));
        assert_eq!(location.fragment(), Some("reply"));
        assert_eq!(location.href(), "/community/articles/42?tab=comments#reply");
    }

    #[test]
    fn location_resolves_absolute_url_style_and_fragment_targets() {
        let location = Location::new("/community/articles/42?tab=comments");
        assert_eq!(
            location.resolve("../events?page=2").href(),
            "/community/events?page=2"
        );
        assert_eq!(
            location.resolve("#reply").href(),
            "/community/articles/42?tab=comments#reply"
        );
        assert_eq!(location.resolve("/store").href(), "/store");
    }

    #[test]
    fn percent_decode_rejects_invalid_or_non_utf8_values() {
        assert_eq!(percent_decode("hello%20world"), Some("hello world".into()));
        assert_eq!(percent_decode("%GG"), None);
        assert_eq!(percent_decode("%FF"), None);
    }

    #[test]
    fn dynamic_path_segments_round_trip_without_changing_hierarchy() {
        let value = "文章/42 + Rust";
        let encoded = Location::encode_path_segment(value);
        assert_eq!(encoded, "%E6%96%87%E7%AB%A0%2F42%20%2B%20Rust");
        assert_eq!(percent_decode(&encoded).as_deref(), Some(value));
    }
}
