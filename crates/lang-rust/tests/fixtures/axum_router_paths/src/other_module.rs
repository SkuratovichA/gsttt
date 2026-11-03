use axum::routing::get;
use other_web::routing::Router;

pub fn mount(router: Router) -> Router {
    router.route("/other_module", get(|| async {}))
}
