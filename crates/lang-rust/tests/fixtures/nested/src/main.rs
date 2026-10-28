mod bookings;

use axum::Router;

fn app() -> Router {
    Router::new().nest("/api/bookings", bookings::router())
}

fn main() {
    let _ = app();
}
