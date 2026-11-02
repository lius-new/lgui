mod builder;
mod outlet;
mod redirect;
mod route;

pub use builder::{DeclarativeRouter, create_router};
pub use outlet::outlet;
pub use redirect::redirect;
pub use route::{IntoRoutes, Route, index, layout, not_found, route, scope};

#[path = "declarative_test.rs"]
#[cfg(test)]
mod tests;
