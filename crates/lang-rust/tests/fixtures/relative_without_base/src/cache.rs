use memcache::Client;

pub struct Store {
    cache: Client,
}

impl Store {
    pub fn field(&self) -> Option<String> {
        self.cache.get("users")
    }
}

pub fn parameter(client: &Client) -> Option<String> {
    client.get("users")
}

pub fn local() -> Option<String> {
    let client: Client = connect();
    client.get("users")
}
