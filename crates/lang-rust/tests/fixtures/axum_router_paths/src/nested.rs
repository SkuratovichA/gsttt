use axum::routing::{get, Router};

pub fn mount(router: Router) -> Router {
    router.route("/nested", get(|| async {}))
}
