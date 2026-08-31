#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteAction {
    Navigate,
    Replace,
    Back,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteChange<R> {
    pub previous: R,
    pub current: R,
    pub action: RouteAction,
}
