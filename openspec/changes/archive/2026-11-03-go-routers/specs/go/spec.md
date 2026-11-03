## ADDED Requirements

### Requirement: Router routes
Where `go.mod` requires gin or echo, an upper-case verb (`GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `OPTIONS`, `HEAD`, `Any`) called with a path and a handler SHALL serve that path, on any receiver. A group SHALL give the router it returns the prefix of its receiver plus its argument, for gin, echo and fiber. A verb with one argument SHALL be no route.

#### Scenario: A verb on a gin engine serves its route
- **WHEN** a file of a project that requires gin calls `r.GET("/health", h)` on an engine, `s.engine.POST("/orders", h)` on a field whose type is declared in another file, and `probe.GET("/health")`
- **THEN** the first serves `GET /health` and the second `POST /orders`
- **AND** the third is no route and not unresolved

#### Scenario: A group adds its prefix
- **WHEN** a file calls `v1 := r.Group("/v1")`, `v1.GET("/users/:id", h)`, `users := v1.Group("/users")` and `users.DELETE("/:id", h)` on gin, and the same shapes on echo and fiber
- **THEN** the routes are `GET /v1/users/{}` and `DELETE /v1/users/{}`, and each framework serves its group routes under the prefixes of every group above them

### Requirement: A verb given as text
`Handle` on a gin router and `Add` on an echo or fiber router SHALL serve the path in their second argument under the method in their first. A `net/http` pattern with no `/` SHALL serve nothing, so the verb of such a call is no route.

#### Scenario: A verb given as text serves its route
- **WHEN** a file of a project that requires gin calls `r.Handle("GET", "/handled", h)` on an engine
- **THEN** it serves `GET /handled`
- **AND** no route has the path `GET`, and the call is no unread site

### Requirement: Routers of chi and fiber
Where `go.mod` requires chi or fiber, a verb SHALL serve its route only on a receiver that the file shows to be a router: made by `chi.NewRouter`, `chi.NewMux` or `fiber.New`, or held in a parameter, field or variable typed as the router. A chi or fiber `Route` closure SHALL give the router it receives the prefix, `Group` of chi none, and `With(...)` SHALL leave the router as it is.

#### Scenario: A chi verb on a router it can see serves its route
- **WHEN** a file calls `r.Get("/ping", h)` on `r := chi.NewRouter()`, `r.Method("PUT", "/things", h)` on it, and `r.Put("/typed", h)` on a parameter typed `chi.Router`
- **THEN** it serves `GET /ping`, `PUT /things` and `PUT /typed`

#### Scenario: A chi closure adds its prefix
- **WHEN** a file calls `r.Route("/v1", func(r chi.Router) { r.Get("/users/{id}", h); r.Group(func(r chi.Router) { r.Post("/users", h) }); r.With(auth).Delete("/users/{id:[0-9]+}", h) })`
- **THEN** it serves `GET /v1/users/{}`, `POST /v1/users` and `DELETE /v1/users/{}`

#### Scenario: A Get on what is no router is no route
- **WHEN** a file of a project that requires chi calls `cache.Get("/k", fill)` and `client.Get("/x", nil)` on a cache and an `http.Client`
- **THEN** neither is a route, and neither is unresolved

### Requirement: Gorilla routes
Where `go.mod` requires gorilla/mux, `PathPrefix("/api").Subrouter()` SHALL give the router it returns the prefix of its receiver plus its argument, and `.Methods("GET", "POST")` on a route SHALL limit it to those methods, one route for each, in place of every method.

#### Scenario: A gorilla sub-router adds its prefix
- **WHEN** a file calls `api := r.PathPrefix("/api").Subrouter()`, `api.HandleFunc("/users/{id}", h).Methods("GET", "POST")` and `r.HandleFunc("/health", h)`
- **THEN** it serves `GET /api/users/{}`, `POST /api/users/{}` and every method of `/health`

### Requirement: Unread prefix
A group or sub-router whose prefix is not text SHALL give its routes no fact, and each such route SHALL count as one unread site.

#### Scenario: A group whose prefix is no text serves no route
- **WHEN** a file of a project that requires gin calls `g := r.Group(p)` with `p` a parameter and then `g.GET("/x", h)`
- **THEN** no route is served and the Go coverage counts one unread site, the line of `g.GET`

### Requirement: Packs follow go.mod
A router pack SHALL read a file only where `go.mod` requires its framework.

#### Scenario: A pack reads nothing where go.mod does not require its framework
- **WHEN** a file imports gin and chi and calls `r.GET("/health", h)` and `c.Get("/ping", h)`, and `go.mod` requires neither
- **THEN** nothing is served and nothing is unresolved
