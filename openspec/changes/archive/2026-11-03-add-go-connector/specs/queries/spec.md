# Spec Delta

## ADDED Requirements

### Requirement: Go answers name declarations
A Go function or method SHALL be a name that `impact --name` asks about: the answer lists the files that call it and the declarations in them, and a caller of another function or another method of the same type is not listed.

#### Scenario: A Go caller of a function is listed and a caller of another function is not
- **WHEN** `invoice/header.go` declares `Header`, `invoice/footer.go` declares `Footer`, `report.go` calls `invoice.Header()` in `Print` and `ship.go` calls `invoice.Footer()`, and `impact invoice/header.go --name Header` runs
- **THEN** the answer lists `report.go` with the caller `Print`, and does not list `ship.go`

#### Scenario: A Go caller of a method is told apart from a caller of another method
- **WHEN** type `Service` has the methods `Place` and `Cancel`, `Order` takes a `*invoice.Service` and calls `Place`, `Refund` takes one and calls `Cancel`, and `impact invoice/service.go --name Service.Place` runs
- **THEN** the callers are `Order` only
- **AND** with `--name Service.Cancel` they are `Refund` only

#### Scenario: A Go method declared in a file of its own is found through its type
- **WHEN** `invoice/charge.go` holds only `func (s *Service) Charge() {}`, `invoice/service.go` declares `Service`, `order.go` calls `Charge` on an `*invoice.Service`, and `impact invoice/charge.go --name Service.Charge` runs
- **THEN** the answer lists `order.go`

#### Scenario: A Go client meets a route served in another language
- **WHEN** an Express app serves `GET /api/users` and a Go file calls `http.Get(base + "/api/users")`
- **THEN** `contract "http GET /api/users"` lists the Express file as serving and the Go file as calling
- **AND** `impact` of the Express file lists the Go file
