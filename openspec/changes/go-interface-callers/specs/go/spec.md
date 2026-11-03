## ADDED Requirements

### Requirement: A type satisfies an interface
The link SHALL give a type, in each file that declares it, an embedding in it or a method of it, the interface of the project as a head when the methods of the type, its package's and those of the types it embeds, hold every method of the interface in the same shape. A type that shares only a name with a method of the interface SHALL be no pair. Names inside a function type SHALL not count, and a type that lends a method to the satisfying one SHALL be a head too.

#### Scenario: A method is reached through an interface its type satisfies
- **WHEN** `store/pg.go` has `func (p *PG) Get(id string) (*User, error)`, `svc/svc.go` has `type Store interface { Get(id string) (*User, error) }` and `svc/use.go` calls `s.Get("x")` on a `Store`
- **THEN** impact of `store/pg.go` for `PG.Get` lists `svc/use.go`

#### Scenario: A caller through an interface is listed by impact
- **WHEN** the `store`, `svc` and `Cache` files of the first scenario are indexed and impact is asked about `PG.Get` and about `Cache.Get`
- **THEN** the first answer lists `svc/use.go` and the second does not

#### Scenario: A method of another signature is no implementation
- **WHEN** `Cache.Get(key int) string` is declared beside `Store` with `Get(id string) (*User, error)`
- **THEN** impact for `Cache.Get` does not list `svc/use.go`

#### Scenario: A type needs every method of the interface
- **WHEN** an interface has `Get` and `Put`, one type has `Get` alone and another has both
- **THEN** only the second type has the interface as a head

#### Scenario: The methods of a package count together
- **WHEN** a type has `Get` in one file and `Put` in another, and an interface has both
- **THEN** the interface is a head in both files

#### Scenario: An embedded type lends its methods
- **WHEN** `type Wrap struct { Local }` and `type T struct { lib.Base }` embed a type with `Get`, declare `Put` themselves, and an interface has `Get` and `Put`
- **THEN** the interface is a head of `Wrap` and of `T`, and of each embedded type that lends one of its methods

#### Scenario: A type that lends its methods is a head in its own files
- **WHEN** `Base` declares `Name` in `base.go`, `Job` embeds `Base` and declares `Run`, and `Task` has `Name` and `Run`
- **THEN** `Task` is a head of `Base` in `base.go` and of `Job` in `job.go`

#### Scenario: Names inside a function type do not count
- **WHEN** a type has `Walk(fn func(path string, n int) error) error` and an interface has `Walk(visit func(p string, count int) error) error`
- **THEN** the interface is a head of the type, and not when the function type has another count or type

### Requirement: Interfaces that are no head
An interface with a union or a constraint term, and one that lists no method of its own or of the interfaces it embeds, SHALL be no head of any type.

#### Scenario: An interface with a union is no head
- **WHEN** an interface lists `~int | ~string` or `int` beside `Get() int` and a type declares `Get() int`
- **THEN** the type has no head

#### Scenario: An interface that embeds a generic one is a head
- **WHEN** `IntStore` embeds `Getter[int]` and lists `Size() int`, and a type has `Get(k string) int` and `Size() int`
- **THEN** `IntStore` is a head of the type

#### Scenario: An interface that knows no method is no head
- **WHEN** an interface is `interface{}` or embeds only such an interface
- **THEN** no type has it as a head

### Requirement: A package is a directory and its clause
The link SHALL take the files of one directory with the same name in the package clause as one package, so that `foo` and `foo_test` of a directory are two, and the methods of a type of one SHALL not count for a type of that name in the other.

#### Scenario: Package clause separates a test package
- **WHEN** `foo/t.go` of package `foo` and `foo/t_test.go` of package `foo_test` each declare `T` with a `Get` of another result, and an interface asks for the first
- **THEN** only `foo/t.go` has the interface as a head
