-- HTTP requests with `httpx`: the module-level verbs, and the verbs and `request(method, url)` on an
-- `httpx.Client()` or `httpx.AsyncClient()` held in a variable of the same file, behind its `base_url`.
local client = {
  from = { "httpx.Client", "httpx.AsyncClient" },
  key = "$base_url",
}

return {
  name = "httpx",
  language = "python",
  detect = { "httpx" },
  calls = {
    {
      callee = {
        "httpx.get",
        "httpx.post",
        "httpx.put",
        "httpx.patch",
        "httpx.delete",
        "httpx.head",
        "httpx.options",
      },
      kind = "http",
      role = "calls",
      key = "$arg1|$url",
      method = "$callee",
    },
    {
      callee = "httpx.request",
      kind = "http",
      role = "calls",
      key = "$arg2|$url",
      method = "$arg1|$method",
    },
    {
      callee = {
        "*.get",
        "*.post",
        "*.put",
        "*.patch",
        "*.delete",
        "*.head",
        "*.options",
        "*.*.get",
        "*.*.post",
        "*.*.put",
        "*.*.patch",
        "*.*.delete",
        "*.*.head",
        "*.*.options",
      },
      kind = "http",
      role = "calls",
      key = "$arg1|$url",
      method = "$callee",
      receiver = client,
    },
    {
      callee = { "*.request", "*.*.request" },
      kind = "http",
      role = "calls",
      key = "$arg2|$url",
      method = "$arg1|$method",
      receiver = client,
    },
  },
}
