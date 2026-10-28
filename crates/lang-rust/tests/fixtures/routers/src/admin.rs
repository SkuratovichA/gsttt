use axum::routing::get;
use axum::Router;

async fn list() {}

pub fn router() -> Router {
    let users = Router::new().route("/users", get(list));
    Router::new().nest("/v1", users)
}
