# Spec Delta

## ADDED Requirements

### Requirement: No read on a value of a type outside the module
A Go file SHALL record no member read on a chain of fields, calls, indexes and parentheses that stands on a value of a type of a package outside the repository: a name that holds one, a variable of such a package, a field a struct of the file writes with one, or a call of a function of the standard library. A type written with type arguments SHALL not count as outside. The operands and arguments of the chain SHALL still be read.

#### Scenario: A chain on a value of a standard library type reads no member
- **WHEN** a file has `w.Header().Get("x")` with `w http.ResponseWriter`, `r.Header.Get("x")` with `r *http.Request`, and `c.Request.Header.Set("a", "b")` with a struct `Context` that writes `Request *http.Request`
- **THEN** the file records no read

#### Scenario: A chain on a value that may be of a type here is read
- **WHEN** a file has `p.Load().Place()` with `p atomic.Pointer[T]`
- **THEN** the file records the read `Place`

#### Scenario: The arguments of a chain on an outside value are read
- **WHEN** a file has `w.Header().Set("a", svc.Name())` with `w http.ResponseWriter` and `svc` of unknown type
- **THEN** the file records the read `Name` and no other

### Requirement: A call of the standard library gives a value outside the module
A Go file SHALL treat a name bound to a call of a function of the standard library as a value of a type outside the module. The standard library SHALL be an import outside the repository whose path has no dot in its first segment, bar `slices`, `maps`, `cmp`, `iter`, `sync`, `sync/atomic`, `unique` and `weak`. A call of any other package, a name assigned after it is declared and a type-asserted value SHALL not be one.

#### Scenario: A name bound to a call of the standard library stands outside the module
- **WHEN** a file has `req, _ := http.NewRequest(...)` then `req.Header.Set("a", "b")`, `w := httptest.NewRecorder()` then `w.Header().Get("x")`, and `file, err := os.CreateTemp("", "")` then `file.Name()`
- **THEN** the file records no read

#### Scenario: A value that no call of the standard library gives is read
- **WHEN** a file has `x := lo.Must(y)` with `lo` a third-party import, `x := slices.Max(xs)`, and `req, _ := http.NewRequest(...)` followed by `req = svc.Make()`, and reads `Place` or `Set` on each
- **THEN** the file records each of those reads

#### Scenario: A type-asserted value stands on the asserted type
- **WHEN** a file has `v := ctx.Value(k).(*T)` and `v.Place()`
- **THEN** the file records no read and uses `T.Place`
