-- axios: calls on the default client, on a client made by axios.create (its baseURL is the prefix) and axios(config).
return {
  name = "axios",
  language = "ts",
  detect = { "axios" },
  calls = {
    {
      callee = {
        "axios.get", "axios.post", "axios.put", "axios.patch", "axios.delete", "axios.head", "axios.options",
      },
      kind = "http",
      role = "calls",
      key = "$arg1",
      method = "$callee",
    },
    {
      callee = {
        "*.get", "*.post", "*.put", "*.patch", "*.delete", "*.head", "*.options",
        "*.*.get", "*.*.post", "*.*.put", "*.*.patch", "*.*.delete", "*.*.head", "*.*.options",
      },
      kind = "http",
      role = "calls",
      key = "$arg1",
      method = "$callee",
      receiver = { from = "axios.create", key = "$arg1.baseURL" },
    },
    {
      callee = "axios",
      kind = "http",
      role = "calls",
      key = "$arg1.url|$arg1",
      method = "$arg1.method|$arg2.method",
    },
  },
}
