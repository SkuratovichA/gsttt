-- FastAPI routes. A route belongs to the router or application it is declared on, whose `prefix` comes first.
-- `include_router(router, prefix=...)` is not a rule: the connector reads it and `link` adds the prefix across files.
local receiver = {
  from = { "FastAPI", "fastapi.FastAPI", "APIRouter", "fastapi.APIRouter" },
  key = "$prefix",
}

return {
  name = "fastapi",
  language = "python",
  detect = { "fastapi" },
  decorators = {
    {
      decorator = { "*.get", "*.post", "*.put", "*.patch", "*.delete", "*.options", "*.head" },
      kind = "http",
      role = "serves",
      key = "$arg1|$path",
      method = "$callee",
      receiver = receiver,
    },
    {
      decorator = "*.api_route",
      kind = "http",
      role = "serves",
      key = "$arg1|$path",
      method = "$methods|*",
      receiver = receiver,
    },
  },
}
