package main

import "github.com/gofiber/fiber/v2"

func routes(cache Cache) {
	app := fiber.New()
	app.Get("/health", health)
	api := app.Group("/api")
	api.Get("/users/:id", getUser)
	v2 := api.Group("/v2")
	v2.Post("/users", createUser)
	app.Route("/admin", func(router fiber.Router) {
		router.Get("/stats", stats)
	})
	cache.Get("/k", fill)
}

var shared *fiber.App

func init() {
	shared = fiber.New()
	shared.Get("/shared", health)
	todos := shared.Group("/todo").Use(auth)
	todos.Post("/create", createTodo)
}
