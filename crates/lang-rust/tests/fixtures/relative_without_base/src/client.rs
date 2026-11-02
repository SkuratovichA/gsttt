pub async fn bare() -> Result<String, reqwest::Error> {
    reqwest::get("users").await?.text().await
}

pub async fn held() -> Result<String, reqwest::Error> {
    let client = reqwest::Client::new();
    client.get("users").send().await?.text().await
}

pub async fn absolute() -> Result<String, reqwest::Error> {
    reqwest::get("https://a.test/x").await?.text().await
}
