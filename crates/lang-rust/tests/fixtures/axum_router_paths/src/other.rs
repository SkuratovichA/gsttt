use axum::routing::get;
use other_web::Router;

pub fn mount(router: Router) -> Router {
    router.route("/other", get(|| async {}))
}
