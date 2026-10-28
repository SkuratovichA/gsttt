-- reqwest: `reqwest::get(url)`, and `get`, `post`, `put`, `patch`, `delete`, `head` and `request(Method::POST, url)` on a
-- client. `get` is also a method of a map and a cache, so a method call counts only on a receiver the same file shows to
-- be a client: bound to `Client::new()` or to a `Client::builder()` chain, or a field, parameter or static whose type
-- names `Client`. `ClientBuilder::new()` makes a client the same way `Client::builder()` does.
local clients = {
  "reqwest.Client.new", "reqwest.Client.builder", "Client.new", "Client.builder",
  "reqwest.blocking.Client.new", "reqwest.blocking.Client.builder", "blocking.Client.new", "blocking.Client.builder",
  "reqwest.ClientBuilder.new", "ClientBuilder.new", "reqwest.blocking.ClientBuilder.new", "blocking.ClientBuilder.new",
}

return {
  name = "reqwest",
  language = "rust",
  detect = { "reqwest" },
  calls = {
    { callee = { "reqwest.get", "reqwest.blocking.get" }, kind = "http", role = "calls", key = "$arg1", method = "GET" },
    {
      callee = { "*.get", "*.post", "*.put", "*.patch", "*.delete", "*.head" },
      kind = "http",
      role = "calls",
      key = "$arg1",
      method = "$callee",
      receiver = { from = clients },
    },
    -- client.request(Method::POST, url): the verb is the last segment of the first argument.
    {
      callee = "*.request",
      kind = "http",
      role = "calls",
      key = "$arg2",
      method = "$arg1.last",
      receiver = { from = clients },
    },
  },
}
