use reqwest::Client;

pub async fn built() -> Result<(), reqwest::Error> {
    let client = Client::builder().user_agent("gsttt").build()?;
    client.post("/bookings").send().await?;
    client
        .request(reqwest::Method::PUT, "/bookings/1")
        .send()
        .await?;
    Ok(())
}
