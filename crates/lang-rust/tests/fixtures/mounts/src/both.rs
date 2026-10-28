use axum::routing::get;
use axum::Router;

async fn x() {}

fn router() -> Router {
    Router::new().route("/x", get(x))
}

pub fn app() -> Router {
    Router::new().nest("/api", router()).merge(router())
}
