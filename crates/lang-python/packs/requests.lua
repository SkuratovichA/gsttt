-- HTTP requests with `requests`: the module-level verbs, `requests.request(method, url)` and the verbs on a
-- `requests.Session()` held in a variable of the same file.
return {
  name = "requests",
  language = "python",
  detect = { "requests" },
  calls = {
    {
      callee = {
        "requests.get",
        "requests.post",
        "requests.put",
        "requests.patch",
        "requests.delete",
        "requests.head",
        "requests.options",
      },
      kind = "http",
      role = "calls",
      key = "$arg1|$url",
      method = "$callee",
    },
    {
      callee = "requests.request",
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
      receiver = { from = { "requests.Session", "requests.session" } },
    },
  },
}
