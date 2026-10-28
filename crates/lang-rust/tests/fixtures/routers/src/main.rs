mod admin;
mod bookings;
mod rings;
mod rooms;

use axum::routing::get;
use axum::Router;

async fn ok() {}

fn health() -> Router {
    Router::new().route("/health", get(ok))
}

fn api() -> Router {
    let reports = Router::new().route("/reports", get(ok));
    Router::new()
        .nest("/bookings", bookings::router())
        .nest("/rooms", rooms::router())
        .merge(reports)
}

fn app() -> Router {
    Router::new()
        .nest("/api", api())
        .nest("/admin", admin::router())
        .nest("/rings", rings::ping())
        .merge(health())
}

fn under(prefix: &str) -> Router {
    Router::new().nest(prefix, bookings::router())
}

fn main() {
    let _ = (app(), under("/x"));
}
