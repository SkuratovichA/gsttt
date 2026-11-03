# Spec Delta

## ADDED Requirements

### Requirement: A method through what a function returns
Asked about a Go method, `impact` SHALL list a file that calls it on a value a function returns, on a type that embeds the type of the method, or on a package-level variable, and SHALL not list a file that reads another method there. A file that names the type and reads no such method SHALL be counted in a note.

#### Scenario: A method called on what a function returns reaches the caller
- **WHEN** `lib/new.go` declares `func NewRouter() *Mux`, `app/uses.go` calls `lib.NewRouter().Get` through a local, `app/other.go` does the same with `Put`, and impact is asked about `Mux.Get` in `lib/mux.go`
- **THEN** impact lists `app/uses.go` and not `app/other.go` or `lib/new.go`
- **AND** a note counts the two files that hold `Mux` and name no `Get`

#### Scenario: A method promoted through embedding reaches the caller
- **WHEN** `Engine` embeds `RouterGroup`, `func New() *Engine` is in the same file, a file of another package calls `e := lib.New(); e.PATCH()`, and impact is asked about `RouterGroup.PATCH`
- **THEN** impact lists that file

#### Scenario: A method through a package variable reaches the caller
- **WHEN** `var V2 = v2{}` is declared, a file calls `lib.V2.Make()`, and impact is asked about `v2.Make`
- **THEN** impact lists that file
