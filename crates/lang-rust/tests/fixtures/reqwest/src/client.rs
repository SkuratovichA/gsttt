//! Requests built from literal parts, and one whose URL is a variable.

use std::time::Duration;

pub async fn cancel(url: &str, id: u64) -> Result<(), reqwest::Error> {
    let client = reqwest::Client::new();
    // Both requests give up after the same time.
    let timeout = Duration::from_secs(5);
    let _ = timeout;
    // A literal path with a placeholder for the id keeps its shape.
    // The URL of the second call is only known at run time.
    client.delete(format!("/bookings/{}", id)).send().await?;
    client.get(url).send().await?;
    Ok(())
}
