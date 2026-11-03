package main

import (
	"net/http"

	"github.com/go-chi/chi/v5"
)

func routes(cache Cache) http.Handler {
	r := chi.NewRouter()
	r.Get("/ping", ping)
	r.Route("/v1", func(r chi.Router) {
		r.Get("/users/{id}", getUser)
		r.Group(func(r chi.Router) {
			r.Post("/users", createUser)
		})
		r.With(auth).Delete("/users/{id:[0-9]+}", deleteUser)
	})
	r.Method("PUT", "/things", putThing)
	cache.Get("/k", fill)
	client := &http.Client{}
	client.Get("/x", nil)
	return r
}

func mount(r chi.Router) {
	r.Put("/typed", typed)
}
