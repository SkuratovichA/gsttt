pub async fn calls(base: &str, id: u64) -> Result<(), reqwest::Error> {
    let client = reqwest::Client::new();
    client.get(format!("{}/{}", base, id)).send().await?;
    client.get(format!("{}/users/{}", base, id)).send().await?;
    Ok(())
}
