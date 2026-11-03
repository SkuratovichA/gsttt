package main

import "github.com/labstack/echo/v4"

func added(e *echo.Echo, h echo.HandlerFunc) {
	e.Add("GET", "/added", h)
}
