package main

import "github.com/gin-gonic/gin"

func handled(r *gin.Engine, verb string, h gin.HandlerFunc) {
	r.Handle("GET", "/handled", h)
	r.Handle(verb, "/any", h)
}
