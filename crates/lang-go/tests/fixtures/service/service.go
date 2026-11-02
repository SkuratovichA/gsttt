package main

import (
	"fmt"
	"net/http"
	"os"

	"github.com/go-chi/chi/v5"
	"example.com/shop/invoice"
	yaml "gopkg.in/yaml.v3"
	_ "example.com/shop/billing"
)

type Service struct {
	header *invoice.Header
	db     DB
}

func (s *Service) Place(h invoice.Header) string {
	t := invoice.NewHeader("1")
	fmt.Println(t.Title())
	_ = chi.NewRouter()
	_ = yaml.Marshal
	_ = os.Getenv("API_URL")
	return s.header.Title()
}

func (s *Service) Cancel() {
	rows, _ := s.db.Query("SELECT id FROM orders")
	_ = rows
	http.Get("/api/users")
}

func main() {
	var s Service
	s.Place(invoice.Header{})
}
