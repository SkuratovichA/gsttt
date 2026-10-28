-- actix-web: routes written as attributes on handlers, as `.route(..)` on an `App` or a `Scope`, and as
-- `web::resource(..).route(..)`. A `web::scope("/prefix").service(handler)` carries a prefix to a handler in another
-- file; the connector follows it when it links files, so it needs no rule here.
return {
  name = "actix",
  language = "rust",
  -- Cargo takes `-` and `_` in a package name to be the same, so a manifest may spell either.
  detect = { "actix-web", "actix_web" },
  calls = {
    -- App::new().route("/bookings", web::get().to(list))
    { callee = "*.route", kind = "http", role = "serves", key = "$arg1", method = "$arg2.method" },
    -- web::resource("/bookings").route(web::get().to(list)): the path is the resource's, the verb the route's.
    {
      callee = "*.route",
      kind = "http",
      role = "serves",
      key = "",
      method = "$arg1.method",
      receiver = { from = { "web.resource", "actix_web.web.resource" }, key = "$arg1" },
    },
  },
  decorators = {
    -- #[get("/{id}")]
    {
      decorator = {
        "get", "post", "put", "patch", "delete", "head",
        "actix_web.get", "actix_web.post", "actix_web.put", "actix_web.patch", "actix_web.delete", "actix_web.head",
      },
      kind = "http",
      role = "serves",
      key = "$arg1",
      method = "$callee",
    },
    -- #[route("/{id}", method = "GET", method = "HEAD")]
    { decorator = { "route", "actix_web.route" }, kind = "http", role = "serves", key = "$arg1", method = "$method" },
  },
}
