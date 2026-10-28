mod imported;

use axum::Router;
use axum::routing::{MethodRouter, get};
use reqwest::ClientBuilder;

pub fn app() -> Router {
    Router::new().route("/health", get(health))
}

async fn health() {}

pub struct Planner;

impl Planner {
    pub fn route(&self, _path: &str, _handler: MethodRouter) {}
}

/// `route` is a method of other types too, and this one is no router.
pub fn plan(planner: &Planner) {
    planner.route("/not-a-route", get(health));
}

pub async fn fetch() {
    let client = ClientBuilder::new().build().unwrap();
    let _ = client.get("https://api.example.com/items").send().await;
}
