package main

import "github.com/gorilla/mux"

func routes() {
	r := mux.NewRouter()
	r.HandleFunc("/health", health)
	api := r.PathPrefix("/api").Subrouter()
	api.HandleFunc("/users/{id}", getUser).Methods("GET", "POST")
	api.HandleFunc("/orders", orders).Methods("PUT")
	v1 := api.PathPrefix("/v1").Subrouter()
	v1.Handle("/files", files)
}
