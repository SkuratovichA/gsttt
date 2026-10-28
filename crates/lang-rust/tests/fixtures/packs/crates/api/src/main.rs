use axum::routing::get;
use axum::Router;

async fn list() {}

fn main() {
    let _ = Router::new().route("/bookings", get(list));
}
