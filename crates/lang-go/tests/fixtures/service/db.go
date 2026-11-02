package main

type DB interface {
	Query(q string, args ...any) (any, error)
}
