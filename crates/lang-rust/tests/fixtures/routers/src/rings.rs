use axum::routing::get;
use axum::Router;

async fn ok() {}

pub fn ping() -> Router {
    Router::new().route("/ping", get(ok)).nest("/pong", pong())
}

pub fn pong() -> Router {
    Router::new().route("/pong", get(ok)).nest("/ping", ping())
}
