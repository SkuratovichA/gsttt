-- Django routes and tables. A `path()` entry serves its route under any method; `path("api/", include("app.urls"))`
-- is not a route but a mount, which the connector reads and `link` composes across files. `re_path()` patterns are
-- regular expressions, not paths, and are only read as mounts.
return {
  name = "django",
  language = "python",
  detect = { "django" },
  calls = {
    {
      callee = { "path", "django.urls.path", "urls.path" },
      kind = "http",
      role = "serves",
      key = "$arg1|$route",
    },
  },
  assigns = {
    {
      target = "db_table",
      kind = "table",
      role = "declares",
      key = "$arg1",
    },
  },
}
