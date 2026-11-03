use axum::routing;

pub fn mount(router: routing::Router) -> routing::Router {
    router.route("/module", routing::get(|| async {}))
}
