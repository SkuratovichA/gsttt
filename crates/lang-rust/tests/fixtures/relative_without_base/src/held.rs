use reqwest::Client;

pub async fn imported(client: &Client) -> Result<String, reqwest::Error> {
    client.get("users").send().await?.text().await
}
