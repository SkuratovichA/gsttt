use reqwest::get;
use std::env::var;

pub async fn fetch() {
    let _ = get("https://api.example.com/imported").await;
    let _ = var("IMPORTED");
}
