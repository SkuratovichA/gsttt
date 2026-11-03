package main

import (
	"github.com/gin-gonic/gin"
	"github.com/go-chi/chi/v5"
)

func routes(r *gin.Engine, c chi.Router) {
	r.GET("/health", health)
	c.Get("/ping", ping)
}
