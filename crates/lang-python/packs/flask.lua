-- Flask routes. A route belongs to the application or blueprint it is declared on, whose `url_prefix` comes first.
-- A route with several methods is read under the first of them: a rule yields one fact per site.
-- `register_blueprint(bp, url_prefix=...)` is not a rule: the connector reads it and `link` adds the prefix across files.
local receiver = {
  from = { "Flask", "flask.Flask", "Blueprint", "flask.Blueprint" },
  key = "$url_prefix",
}

return {
  name = "flask",
  language = "python",
  detect = { "flask" },
  decorators = {
    {
      decorator = "*.route",
      kind = "http",
      role = "serves",
      key = "$arg1|$rule",
      method = "$methods|GET",
      receiver = receiver,
    },
    {
      decorator = { "*.get", "*.post", "*.put", "*.patch", "*.delete" },
      kind = "http",
      role = "serves",
      key = "$arg1|$rule",
      method = "$callee",
      receiver = receiver,
    },
  },
}
