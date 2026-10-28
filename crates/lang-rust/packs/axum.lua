-- axum: `Router::new().route("/bookings/{id}", get(show).delete(destroy))` serves one route per method of the method
-- router, and `any(..)` serves every method. `nest` and `merge` carry a prefix across routers; the connector follows
-- them when it links files, so they need no rule here.
return {
  name = "axum",
  language = "rust",
  detect = { "axum" },
  calls = {
    { callee = "*.route", kind = "http", role = "serves", key = "$arg1", method = "$arg2.method" },
  },
}
