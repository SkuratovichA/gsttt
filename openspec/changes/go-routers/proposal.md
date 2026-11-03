# Proposal: Read the routes of the Go routers

## Why

gsttt reads the routes of `net/http` only. A service on gin, chi, echo, gorilla/mux or fiber serves its routes through a router, so a client that calls them links to nothing and the coverage says only that routers are not read. A verb is not enough to tell a route from other code, though: `cache.Get("/k", v)` and `client.Get("/x", nil)` are not routes, and a reader that takes them for routes adds noise to every answer.

## What Changes

- Go: five rule packs, `gin`, `echo`, `chi`, `fiber` and `gorilla`, each switched on by the framework in `go.mod`. The upper-case verbs of gin and echo serve a route. The verbs of chi and fiber serve a route only on a receiver the file shows to be a router: made by `chi.NewRouter`, `chi.NewMux` or `fiber.New`, or held in a parameter, a field or a variable typed `chi.Router`, `chi.Mux`, `fiber.App` or `fiber.Router`. On any other receiver a verb gives no fact and no unread site.
- Go: a call with a verb and one argument is no route, for every rule that serves a route, `net/http` included.
- Go: `Group("/v1")` on gin, echo and fiber, and `PathPrefix("/api").Subrouter()` on gorilla, give the router they return the prefix of the receiver plus the text of the argument. A chi or fiber `Route("/x", func(r …) {})` gives its closure the prefix, `Group(func(r chi.Router) {})` gives it none, and `r.With(mw).Get` is `r.Get`. A prefix that is not text is unread: a route on it gives no fact and one unread site.
- Go: `.Methods("GET", "POST")` on a gorilla route gives one route per method, in place of every method.
- Core: a path segment that starts with `*` (`*path`) is a parameter, so a catch-all of gin and echo meets the path a client calls.
- The coverage of a router pack names what it cannot see.
- Versions: `READER_VERSION` of the Go connector goes from 3 to 4, because the facts of an unchanged file change. `FACT_FORMAT` goes from 18 to 19, because the core makes a different key from the same text.

## Impact

- Capabilities `go` and `indexing`.
- `crates/lang-go/src/routers.rs`, `site.rs`, `walk.rs`, `scope.rs`, `provides.rs`, `project.rs`, the packs in `crates/lang-go/packs/`, and `normalize_http_path` in `crates/core/src/contract.rs`.
- A Go tree that does not require one of the frameworks reads as before, except that a `Handle` or `HandleFunc` call with one argument is no route.
