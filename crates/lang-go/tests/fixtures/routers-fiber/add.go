package main

import "github.com/gofiber/fiber/v2"

func added(app *fiber.App, h fiber.Handler) {
	app.Add("GET", "/added", h)
}
