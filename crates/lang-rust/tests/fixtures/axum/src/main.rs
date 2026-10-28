use axum::routing::{any, get, post, trace};
use axum::Router;

async fn show() {}
async fn destroy() {}
async fn create() {}
async fn probe() {}
async fn fallback() {}

fn app() -> Router {
    Router::new()
        .route("/bookings/{id}", get(show).delete(destroy))
        .route("/bookings", post(create))
        .route("/probe", trace(probe))
        .route("/anything", any(fallback))
}

fn computed(path: &str) -> Router {
    Router::new().route(path, get(show))
}

fn main() {
    let _ = app();
    let _ = computed("/x");
}
