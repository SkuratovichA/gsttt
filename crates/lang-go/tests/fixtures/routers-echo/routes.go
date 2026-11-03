package main

import "github.com/labstack/echo/v4"

func routes() {
	e := echo.New()
	e.GET("/health", health)
	api := e.Group("/api")
	api.GET("/items/:id", getItem)
	v2 := api.Group("/v2")
	v2.POST("/items", createItem)
}
