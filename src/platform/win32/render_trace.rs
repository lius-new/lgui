use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TraceCategory {
    Animation,
    Dirty,
    Present,
    Input,
    DirtyDetail,
    RegionDetail,
    Backend,
    Error,
}

impl TraceCategory {
    fn key(self) -> &'static str {
        match self {
            Self::Animation => "animation",
            Self::Dirty => "dirty",
            Self::Present => "present",
            Self::Input => "input",
            Self::DirtyDetail => "dirty-detail",
            Self::RegionDetail => "region-detail",
            Self::Backend => "backend",
            Self::Error => "error",
        }
    }
}

pub fn enabled(category: TraceCategory) -> bool {
    if let Some(filter) = trace_filter() {
        return filter.all || filter.categories.iter().any(|item| item == category.key());
    }
    std::env::var_os("LGUI_TRACE_FIRST_FRAME").is_some()
}

pub fn duration_enabled(label: &str) -> bool {
    if let Some(filter) = trace_filter() {
        return filter.all
            || filter.categories.iter().any(|item| item == "duration")
            || filter.categories.iter().any(|item| label.starts_with(item));
    }
    std::env::var_os("LGUI_TRACE_FIRST_FRAME").is_some()
}

pub fn duration_detail_enabled(label: &str) -> bool {
    if let Some(filter) = trace_filter() {
        return filter.all || filter.categories.iter().any(|item| label.starts_with(item));
    }
    false
}

struct TraceFilter {
    all: bool,
    categories: Vec<String>,
}

fn trace_filter() -> Option<&'static TraceFilter> {
    static FILTER: OnceLock<Option<TraceFilter>> = OnceLock::new();
    FILTER
        .get_or_init(|| {
            let value = std::env::var("LGUI_TRACE_UI").ok()?;
            let categories: Vec<String> = value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_ascii_lowercase)
                .collect();
            Some(TraceFilter {
                all: categories.iter().any(|item| item == "all"),
                categories,
            })
        })
        .as_ref()
}
