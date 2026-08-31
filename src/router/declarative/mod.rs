mod builder;
mod outlet;
mod redirect;
mod route;

pub use builder::{create_router, DeclarativeRouter};
pub use outlet::outlet;
pub use redirect::redirect;
pub use route::{index, layout, not_found, route, scope, IntoRoutes, Route};

#[cfg(test)]
mod tests;
