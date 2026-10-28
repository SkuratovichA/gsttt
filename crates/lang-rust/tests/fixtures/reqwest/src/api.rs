use std::collections::HashMap;

pub struct Api {
    http: reqwest::Client,
    base: String,
}

impl Api {
    pub async fn booking(&self, id: u64) -> Result<String, reqwest::Error> {
        self.http
            .get(format!("{}/bookings/{}", self.base, id))
            .send()
            .await?
            .text()
            .await
    }
}

pub fn cached(cache: &HashMap<String, String>) -> Option<&String> {
    cache.get("/bookings")
}

pub async fn status() -> Result<String, reqwest::Error> {
    reqwest::get("https://example.com/status").await?.text().await
}
