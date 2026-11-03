# Spec Delta

## MODIFIED Requirements

### Requirement: What a declaration uses
A declaration SHALL use the functions, types and values it names, a type by its name in every position, by package for a name read through an import, and the method it calls through a receiver, a parameter, a local value made with a literal, `&T{}` or `new(T)`, a field of a struct of the file or a range variable whose type the file shows. A value a call returns SHALL be read, not typed. A name a local shadows SHALL not be a use of the package.

#### Scenario: A function uses what it calls by package and by name
- **WHEN** a function calls `helper()`, `fmt.Println` and `str.ToUpper` where `str` is an alias, and reads `name`
- **THEN** it uses `fmt.Println`, `helper`, `name` and `str.ToUpper`

#### Scenario: A parameter's type is a use of the type and of no member
- **WHEN** a function takes `h Header` and `i inv.Invoice` and names a result of type `Footer`
- **THEN** it uses `Header`, `Footer` and `inv.Invoice` as types, with no member

#### Scenario: A method read through a receiver is that method
- **WHEN** method `A` of `S` calls `s.B()` on its receiver
- **THEN** `S.A` uses `S.B`

#### Scenario: A method read through a parameter, a local, a constructor and a range variable is that method
- **WHEN** `h.Title()` is called on a parameter, on a value made with `Header{}`, `&Header{}`, `new(Header)`, a range over a slice, an index, a copy, a type assertion or a `var` of the type
- **THEN** each function uses `Header.Title`, or `inv.Header.Title` for the package-qualified type

#### Scenario: A method read through the first result of a constructor is that method
- **WHEN** `svc, err := NewService(cfg)` or `svc, err := inv.NewService(cfg)` is followed by `svc.Charge()`
- **THEN** the function uses `NewService` or `inv.NewService` and reads `Charge`, and uses no `Service.Charge`

#### Scenario: A method read through a field of a struct of the file is that method
- **WHEN** a method calls `s.db.Query()` and `s.store.Save()` where the struct declares `db *DB` and `store Store`
- **THEN** it uses `DB.Query`, `Store.Save`, `Service.db` and `Service.store`

#### Scenario: A use of a type is no use of its members
- **WHEN** a file has `type byName []*Command`, `var hook func(*Command)` and `&Command{}`
- **THEN** each uses `Command` and no member

#### Scenario: A value passed on is a use of the type
- **WHEN** a function makes `Header{}` and `&Footer{}` and passes them to `consume`
- **THEN** it uses `Header`, `Footer` and `consume`

#### Scenario: A method read through a value of no known type is not recorded
- **WHEN** a function calls `x.Run()` on the result of `mystery()` and `y.Stop()` on the result of `x.Next()`
- **THEN** it uses `mystery` and no method

#### Scenario: A shadowing local is no use of the package
- **WHEN** a function declares `fmt := Printer{}` and calls `fmt.Println()` and another function calls `fmt.Println()` on the imported package
- **THEN** the first uses `Printer` and `Printer.Println`, the second `fmt.Println`

## ADDED Requirements

### Requirement: What takes the members of a type
The connector SHALL record, for a name, the type whose members it takes: a function the head of its first result, a struct and an interface each embedded type, an alias its type, and a package-level `var` the type or the function it holds. A defined type, a method, a `const`, `_`, a predeclared type and a type parameter SHALL record nothing. A value a call returns SHALL be read, not typed.

#### Scenario: A function takes the members of its first result
- **WHEN** a file declares `func NewRouter() *Mux`, `func F() (*Mux, error)`, `func L() []*Mux` and `func Q() *pkg.T`, and also `func E() error`, `func G[T any]() T` and a method
- **THEN** the first four are heirs of `Mux`, `Mux`, `Mux` and `pkg.T`, and the others are heirs of nothing

#### Scenario: An embedded type gives its members
- **WHEN** a struct embeds `RouterGroup` and `*pkg.Base`, an interface embeds `B` and `pkg.C`, and another interface is `~int | string`
- **THEN** the struct is an heir of `RouterGroup` and `pkg.Base`, the interface of `B` and `pkg.C`, and the union of nothing

#### Scenario: An alias takes the members of its type
- **WHEN** a file declares `type A = T` and `type D T`
- **THEN** `A` is an heir of `T` and `D` of nothing

#### Scenario: A package variable takes the members of what it holds
- **WHEN** a file declares `var X T`, `var X = T{}`, `var X = &T{}`, `var X = new(T)`, `var X = F()` and `var X = pkg.F()`, and also `const C = T(1)` and `var _ = F()`
- **THEN** the variables are heirs of `T`, `T`, `T`, `T`, `F` and `pkg.F`, and the constant and `_` are heirs of nothing

#### Scenario: A value a call returns is read
- **WHEN** a function calls `x := pkg.F(); x.M()` and `pkg.F().N()`
- **THEN** it uses `pkg.F` and reads `M` and `N`
