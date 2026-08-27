use std::collections::HashSet;

use super::{location::percent_decode, Location};

pub(super) struct MatchStep {
    pub next_offset: usize,
    pub can_end: bool,
    pub score: u32,
    pub pathname: Option<String>,
    pub params: Vec<(String, String)>,
    pub location: Option<Location>,
}

#[derive(Clone)]
pub(super) struct PathPattern {
    pub source: &'static str,
    absolute: bool,
    segments: Vec<PathSegment>,
}

#[derive(Clone)]
enum PathSegment {
    Static(String),
    Parameter(String),
    Wildcard(String),
}

impl PathPattern {
    pub fn parse(source: &'static str) -> Self {
        assert!(!source.is_empty(), "route patterns cannot be empty");
        assert!(
            !source.contains('?') && !source.contains('#'),
            "route patterns cannot contain a query or fragment"
        );
        let absolute = source.starts_with('/');
        let raw_segments = source
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>();
        let mut names = HashSet::new();
        let mut segments = Vec::with_capacity(raw_segments.len());
        for (index, segment) in raw_segments.iter().enumerate() {
            if let Some(name) = segment.strip_prefix(':') {
                assert!(!name.is_empty(), "route parameter names cannot be empty");
                assert!(names.insert(name), "duplicate route parameter `{name}`");
                segments.push(PathSegment::Parameter(name.to_owned()));
            } else if let Some(name) = segment.strip_prefix('*') {
                assert!(!name.is_empty(), "route wildcard names cannot be empty");
                assert!(names.insert(name), "duplicate route parameter `{name}`");
                assert!(
                    index + 1 == raw_segments.len(),
                    "route wildcards must be the final segment"
                );
                segments.push(PathSegment::Wildcard(name.to_owned()));
            } else {
                assert!(
                    !segment.contains('*') && !segment.contains(':'),
                    "route parameter markers must start a segment"
                );
                segments.push(PathSegment::Static((*segment).to_owned()));
            }
        }
        Self {
            source,
            absolute,
            segments,
        }
    }

    pub fn match_location(&self, location: &Location, offset: usize) -> Option<MatchStep> {
        if self.absolute && offset != 0 {
            return None;
        }
        let location_segments = location.segments();
        let mut cursor = offset;
        let mut params = Vec::new();
        // Every declared path outranks a fallback, including the root pattern `/`.
        let mut score = 1;
        for segment in &self.segments {
            match segment {
                PathSegment::Static(expected) => {
                    if location_segments.get(cursor).copied()? != expected {
                        return None;
                    }
                    cursor += 1;
                    score += 100;
                }
                PathSegment::Parameter(name) => {
                    let value = percent_decode(location_segments.get(cursor).copied()?)?;
                    params.push((name.clone(), value));
                    cursor += 1;
                    score += 10;
                }
                PathSegment::Wildcard(name) => {
                    let value = location_segments[cursor..]
                        .iter()
                        .map(|segment| percent_decode(segment))
                        .collect::<Option<Vec<_>>>()?
                        .join("/");
                    params.push((name.clone(), value));
                    cursor = location_segments.len();
                    score += 1;
                }
            }
        }
        let pathname = if cursor == 0 {
            "/".to_owned()
        } else {
            format!("/{}", location_segments[..cursor].join("/"))
        };
        Some(MatchStep {
            next_offset: cursor,
            can_end: cursor == location_segments.len(),
            score,
            pathname: Some(pathname),
            params,
            location: Some(location.clone()),
        })
    }
}
