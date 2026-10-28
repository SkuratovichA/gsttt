use axum::routing::get;
use axum::Router;

pub fn router() -> Router {
    Router::new().route("/{id}", get(show))
}

async fn show() {}
