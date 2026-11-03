package main

// Engine is declared here so that routes.go cannot see its type.
type Engine struct{}

type Server struct {
	engine Engine
}
