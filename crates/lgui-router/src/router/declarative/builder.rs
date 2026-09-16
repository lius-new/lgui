use std::{collections::HashSet, sync::Arc};

use crate::application::AppView;

use super::{
    super::{matcher::pattern::MatchStep, Location, PathParams, RouteId, RouteMatch, RouteMatches},
    route::{IntoRoutes, Route, RouteKind},
};

pub struct DeclarativeRouter<R> {
    pub(super) routes: Arc<Vec<Route<R>>>,
}

impl<R> Clone for DeclarativeRouter<R> {
    fn clone(&self) -> Self {
        Self {
            routes: Arc::clone(&self.routes),
        }
    }
}

pub fn create_router<R>(routes: impl IntoRoutes<R>) -> DeclarativeRouter<R>
where
    R: Clone + PartialEq,
{
    let mut routes = routes.into_routes();
    assert!(
        !routes.is_empty(),
        "a router must define at least one route"
    );
    validate_routes(&routes);
    assign_route_ids(&mut routes, &mut 1);
    DeclarativeRouter {
        routes: Arc::new(routes),
    }
}

impl<R> DeclarativeRouter<R>
where
    R: Default + Clone + PartialEq + Send + Sync + 'static,
{
    pub fn matches(&self, target: &R) -> Option<RouteMatches> {
        self.resolve(target).map(|branch| branch.matches.clone())
    }

    pub(super) fn resolve(&self, target: &R) -> Option<ResolvedBranch> {
        let params = PathParams::default();
        let nodes = resolve_routes(&self.routes, target, 0, "/", &params)?.nodes;
        let location = nodes.iter().find_map(|node| node.location.clone());
        let matches = RouteMatches {
            location,
            entries: nodes.iter().map(|node| node.matched.clone()).collect(),
        };
        Some(ResolvedBranch { nodes, matches })
    }
}

pub(super) struct ResolvedNode {
    pub(super) matched: RouteMatch,
    pub(super) view: AppView,
    pub(super) location: Option<Location>,
}

pub(super) struct ResolvedBranch {
    pub(super) nodes: Vec<ResolvedNode>,
    pub(super) matches: RouteMatches,
}

struct ResolvedCandidate {
    nodes: Vec<ResolvedNode>,
    score: u32,
}

fn resolve_routes<R>(
    routes: &[Route<R>],
    target: &R,
    offset: usize,
    parent_pathname: &str,
    parent_params: &PathParams,
) -> Option<ResolvedCandidate>
where
    R: PartialEq,
{
    resolve_route_pass(
        routes,
        target,
        offset,
        parent_pathname,
        parent_params,
        false,
    )
    .or_else(|| resolve_route_pass(routes, target, offset, parent_pathname, parent_params, true))
}

fn resolve_route_pass<R>(
    routes: &[Route<R>],
    target: &R,
    offset: usize,
    parent_pathname: &str,
    parent_params: &PathParams,
    fallback: bool,
) -> Option<ResolvedCandidate>
where
    R: PartialEq,
{
    let mut best: Option<ResolvedCandidate> = None;
    for route in routes {
        if matches!(route.kind, RouteKind::Fallback(_)) != fallback {
            continue;
        }
        let Some(step) = route.try_match(target, offset, parent_pathname) else {
            continue;
        };
        let mut params = parent_params.clone();
        if !params.extend(step.params.clone()) {
            continue;
        }
        let pathname = step
            .pathname
            .clone()
            .unwrap_or_else(|| parent_pathname.to_owned());
        let matched = RouteMatch {
            id: route.id,
            pattern: route.pattern,
            pathname: pathname.clone(),
            params: params.clone(),
            handle: route.handle.clone(),
        };
        let node = ResolvedNode {
            matched,
            view: Arc::clone(&route.view),
            location: step.location,
        };
        let candidate = if let Some(mut children) = resolve_routes(
            &route.children,
            target,
            step.next_offset,
            &pathname,
            &params,
        ) {
            let mut nodes = Vec::with_capacity(children.nodes.len() + 1);
            nodes.push(node);
            nodes.append(&mut children.nodes);
            Some(ResolvedCandidate {
                nodes,
                score: step.score + children.score,
            })
        } else if step.can_end {
            Some(ResolvedCandidate {
                nodes: vec![node],
                score: step.score,
            })
        } else {
            None
        };
        if let Some(candidate) = candidate {
            if best
                .as_ref()
                .map_or(true, |current| candidate.score > current.score)
            {
                best = Some(candidate);
            }
        }
    }
    best
}

impl<R: PartialEq> Route<R> {
    fn try_match(&self, target: &R, offset: usize, parent_pathname: &str) -> Option<MatchStep> {
        match &self.kind {
            RouteKind::Match(matcher) | RouteKind::Fallback(matcher) => matcher(target, offset),
            RouteKind::Layout => Some(MatchStep {
                next_offset: offset,
                can_end: false,
                score: 0,
                pathname: Some(parent_pathname.to_owned()),
                params: Vec::new(),
                location: None,
            }),
        }
    }
}

fn validate_routes<R>(routes: &[Route<R>])
where
    R: Clone + PartialEq,
{
    validate_level(routes, false);
    let mut terminal_patterns = HashSet::new();
    validate_terminal_patterns(routes, "/", &mut terminal_patterns);
}

fn validate_terminal_patterns<R>(
    routes: &[Route<R>],
    parent: &str,
    terminal_patterns: &mut HashSet<String>,
) {
    for route in routes {
        match &route.kind {
            RouteKind::Layout => {
                validate_terminal_patterns(&route.children, parent, terminal_patterns);
            }
            RouteKind::Match(_) if route.pattern == "<index>" => {
                insert_terminal_pattern(parent, terminal_patterns);
            }
            RouteKind::Match(_) => {
                let full = join_route_pattern(parent, route.pattern);
                validate_full_parameter_names(&full);
                let has_index = route
                    .children
                    .iter()
                    .any(|child| child.pattern == "<index>");
                if !has_index {
                    insert_terminal_pattern(&full, terminal_patterns);
                }
                validate_terminal_patterns(&route.children, &full, terminal_patterns);
            }
            RouteKind::Fallback(_) => {
                insert_terminal_pattern(&format!("<not-found>{parent}"), terminal_patterns);
            }
        }
    }
}

fn validate_full_parameter_names(pattern: &str) {
    let mut names = HashSet::new();
    for segment in pattern.split('/') {
        let name = segment
            .strip_prefix(':')
            .or_else(|| segment.strip_prefix('*'));
        if let Some(name) = name {
            assert!(
                names.insert(name),
                "nested route parameter `{name}` shadows an ancestor parameter"
            );
        }
    }
}

fn insert_terminal_pattern(pattern: &str, terminal_patterns: &mut HashSet<String>) {
    let shape = pattern
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            if segment.starts_with(':') {
                ":"
            } else if segment.starts_with('*') {
                "*"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/");
    let shape = format!("/{shape}");
    assert!(
        terminal_patterns.insert(shape.clone()),
        "ambiguous complete route pattern `{shape}`"
    );
}

fn join_route_pattern(parent: &str, pattern: &str) -> String {
    if pattern.starts_with('/') {
        return pattern.to_owned();
    }
    if parent == "/" {
        format!("/{pattern}")
    } else {
        format!("{}/{pattern}", parent.trim_end_matches('/'))
    }
}

fn validate_level<R>(routes: &[Route<R>], inside_path: bool) {
    let mut sibling_patterns = HashSet::new();
    let mut saw_fallback = false;
    for route in routes {
        match &route.kind {
            RouteKind::Layout => {
                assert!(!route.children.is_empty(), "route layouts require children");
            }
            RouteKind::Fallback(_) => {
                assert!(
                    !saw_fallback,
                    "only one not-found route is allowed per level"
                );
                saw_fallback = true;
                assert!(
                    route.children.is_empty(),
                    "not-found routes cannot have children"
                );
            }
            RouteKind::Match(_) => {
                if route.pattern == "<index>" {
                    assert!(inside_path, "index routes require a parent path");
                    assert!(
                        route.children.is_empty(),
                        "index routes cannot have children"
                    );
                } else {
                    let absolute = route.pattern.starts_with('/');
                    let message = if inside_path {
                        "child route patterns must be relative"
                    } else {
                        "root route patterns must be absolute"
                    };
                    assert!(absolute != inside_path, "{message}");
                }
            }
        }
        assert!(
            !saw_fallback || matches!(route.kind, RouteKind::Fallback(_)),
            "the not-found route must be the final sibling"
        );
        if !matches!(route.kind, RouteKind::Layout) {
            assert!(
                sibling_patterns.insert(route.pattern),
                "duplicate sibling route pattern `{}`",
                route.pattern
            );
        }
        let child_inside_path =
            inside_path || matches!(route.kind, RouteKind::Match(_)) && route.pattern != "<index>";
        validate_level(&route.children, child_inside_path);
    }
}

fn assign_route_ids<R>(routes: &mut [Route<R>], next: &mut u64) {
    for route in routes {
        route.id = RouteId(*next);
        *next += 1;
        assign_route_ids(&mut route.children, next);
    }
}
