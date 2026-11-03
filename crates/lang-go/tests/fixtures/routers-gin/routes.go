package main

import "github.com/gin-gonic/gin"

func routes(r *gin.Engine, s *Server) {
	r.GET("/health", health)
	s.engine.POST("/orders", createOrder)
	v1 := r.Group("/v1")
	v1.GET("/users/:id", getUser)
	v1.GET("/files/*path", getFile)
	users := v1.Group("/users")
	users.DELETE("/:id", deleteUser)
	users.Any("/ping", ping)
	probe.GET("/health")
}
