# go Specification

## Purpose
The Go connector reads Go into imports, declarations and contract sites. It parses with `tree-sitter-go`, resolves imports against the module path, the required modules and the standard library, lets the files of one package reach each other by the names they read, and finds environment variables, HTTP routes and requests, and embedded SQL.

## Requirements

### Requirement: Projects
The connector SHALL open one project per `go.mod`. A project SHALL claim the `go` files under its root and its `go.mod`, which it tags as a manifest, a nested `go.mod` SHALL own what is below it, and a `go.mod` with no `module` line SHALL leave the other projects open, its own module path empty.

#### Scenario: A module is a project named by its go.mod
- **WHEN** a repository has a `go.mod` with `module example.com/m`, `main.go` imports `example.com/m/util`, and `util/util.go` declares `Do`
- **THEN** the connector opens one project rooted at the repository root, which claims both files and `go.mod`
- **AND** the import resolves to `util/util.go`

#### Scenario: A nested module owns what is below it
- **WHEN** the root `go.mod` is `example.com/m`, `tools/go.mod` is `example.com/tools`, and `main.go` imports `example.com/tools/gen`
- **THEN** the connector opens projects rooted at the repository root and at `tools`
- **AND** `tools/gen/gen.go` belongs to `tools`, and the import resolves to the files of `tools/gen`

#### Scenario: A go.mod with no module line leaves the other projects
- **WHEN** one `go.mod` has no `module` line and another is `example.com/m`
- **THEN** the run opens both projects, and the second resolves its imports as before

### Requirement: Files that are not read
The connector SHALL leave out files under `vendor`, `testdata`, and directories whose name begins with `.` or `_`, and files whose name begins with `.` or `_`. A file ending in `_test.go` SHALL be tagged as a test.

#### Scenario: Vendored code and test data are not read
- **WHEN** a module holds `vendor/x/x.go`, `testdata/t.go`, `pkg/testdata/t.go`, `_scratch/s.go`, `.hidden/h.go`, `pkg/_skip.go`, `pkg/.skip.go` and `pkg/ok.go`
- **THEN** only `pkg/ok.go` among them is claimed

#### Scenario: A test file is tagged
- **WHEN** a module holds `a.go` and `a_test.go`
- **THEN** `a_test.go` carries the test tag, `a.go` does not, and `go.mod` carries the manifest tag

### Requirement: Imports are resolved by path
An import of the standard library or of `C` SHALL be built in. An import under the module path or under another module of the repository SHALL reach the files of that directory. An import of any other path SHALL be external, named by its first three segments.

#### Scenario: An import of the standard library is built in
- **WHEN** a file imports `fmt`, `net/http` and `C`
- **THEN** all three resolve as built in

#### Scenario: An import of a required module is external
- **WHEN** `go.mod` requires `github.com/go-chi/chi/v5` and `gopkg.in/yaml.v3`, and a file imports them, `github.com/go-chi/chi/v5/middleware` and `github.com/other/lib`
- **THEN** each resolves to an external package, `chi/v5/middleware` to the package of `chi/v5`

#### Scenario: A package of a nested module is reached
- **WHEN** `main.go` of the root module imports a package of a nested module that has `gen.go` and `gen_test.go`
- **THEN** the import reaches `gen.go` and not the test file

### Requirement: An import reaches the files that declare what is read
An import of a package of the repository SHALL reach the files of the directory that declare a name the importing file reads through the package name or alias, and the files that run when the package loads. An import that reads no name, or a package whose name is not the last segment of its path, SHALL reach every file of the directory. Test files SHALL not be reached. A package of a nested module SHALL be narrowed by name like one of the project's own module.

#### Scenario: An import of a package of the module reaches only the files that declare what the file reads
- **WHEN** `a/x.go` declares `Foo`, `a/y.go` declares `Bar`, `a/doc.go` declares nothing, and `main.go` imports the package and calls `a.Foo()`
- **THEN** the import reaches `a/x.go` only

#### Scenario: An import reaches the files that run when the package loads
- **WHEN** `a/boot.go` has `func init() { Bar() }` and `main.go` calls `a.Foo()`
- **THEN** the import reaches `a/boot.go` and `a/x.go`
- **AND** `boot.go` declares nothing and `Bar` is among what it uses on load

#### Scenario: An import of a package of a nested module reaches only the files that declare what is read
- **WHEN** a nested module declares `Foo` in `gen/a.go` and `Bar` in `gen/b.go`, and a file of the root module imports the package and calls `gen.Foo()`
- **THEN** the import reaches `gen/a.go` only

#### Scenario: An import with an alias binds the alias
- **WHEN** `main.go` imports the package as `pkg`, calls `pkg.Foo()` and has a local variable `a`
- **THEN** the import reaches the file that declares `Foo`, and `main` uses `pkg.Foo`

#### Scenario: A package whose name is not the last segment of its path is followed whole
- **WHEN** the files of `lib` declare `package other` and `main.go` imports `example.com/m/lib` and calls `other.Foo()`
- **THEN** the import reaches every file of `lib`

#### Scenario: A file with methods of a type does not pass the type on
- **WHEN** `command.go` declares `Command` and calls `c.Init()`, and `init.go` and `other.go` hold methods of `Command`
- **THEN** `impact init.go --name Command.Init` lists `command.go` and not `other.go`

#### Scenario: An edge of a package carries only the names its target exports
- **WHEN** `a.go` calls `len` and `Helper`, and `b.go` declares `Helper`
- **THEN** the edge from `a.go` to `b.go` names `Helper` and not `len`

#### Scenario: A blank import reaches every file of the package
- **WHEN** `main.go` has `import _ "example.com/m/a"`
- **THEN** the import is a side-effect import and reaches every file of `a`, test files excepted

### Requirement: Files of one package reach each other
A file SHALL reach the siblings of its `package` clause that declare a name it reads, and no other. A file of another package of the directory, such as `foo_test` or a `//go:build ignore` generator, is no sibling, and a non-test file reaches no test file. A file that declares a type reaches the siblings for a member of it that it does not declare itself. A shadowed name, a field key and an undeclared name are no read.

#### Scenario: A file reaches the sibling that declares a name it reads
- **WHEN** `a.go` calls `helper()` and `b.go` of the same package declares it
- **THEN** `a.go` imports `b.go`

#### Scenario: A method called from the file that declares its type
- **WHEN** `command.go` declares `Command`, `Own` and `Run`, `Run` calls `c.Own()` and `c.Init()`, and `init.go` declares `Init`
- **THEN** `command.go` reaches the siblings that may declare `Init`, and `Own` needs no edge

#### Scenario: A file does not reach a sibling it reads nothing from
- **WHEN** two files of one package read no name the other declares
- **THEN** neither imports the other

#### Scenario: A file that is no test does not reach a test file
- **WHEN** `a.go` calls `fixture()`, which only `a_test.go` declares, and `a_test.go` calls `A()`
- **THEN** `a.go` reaches no file, and `a_test.go` reaches `a.go`

#### Scenario: A file of another package of the directory is no sibling
- **WHEN** a directory holds `real.go` and `lib_test.go` of `package lib`, `ext_test.go` of `package lib_test` and `gen.go` of `package main` under `//go:build ignore`
- **THEN** `lib_test.go` reaches `real.go`, and `ext_test.go` and `gen.go` reach no sibling

#### Scenario: A sibling read past a syntax error may declare any name
- **WHEN** `a.go` reads `Helper`, `broken.go` of the package has a syntax error at its top level and declares nothing, and `fine.go` declares `Fine`
- **THEN** `a.go` reaches `broken.go` and not `fine.go`, because a file read past errors may declare any name

#### Scenario: A local name that shadows a sibling's name is no use of it
- **WHEN** `a.go` declares parameters and locals named `count`, `limit`, `total` and `index`, and `b.go` declares package variables of those names
- **THEN** `a.go` reaches no file and its function uses none of them

#### Scenario: A package that declares its own min is read
- **WHEN** `b.go` declares `func min`, `a.go` calls `min(1, 2)` and `c.go` calls `max(1, 2)`
- **THEN** `a.go` reaches `b.go`, and `c.go` reaches no file because `max` is predeclared

#### Scenario: A struct field key is no use
- **WHEN** `a.go` builds `T{name: 1}` and `map[string]int{name: 1}`, and `b.go` declares the constant `name`
- **THEN** the first function uses `T` and the second uses `name`

### Requirement: Declarations
The connector SHALL declare every top-level function, method, type, struct field, interface method, constant and variable with its lines, a method and a field under the name of their type. `init` SHALL declare nothing, and the names it uses SHALL be recorded as run on load. Every top-level name SHALL be exported, whatever its case, and so SHALL the type of a method, so that a method in a file of its own is found through its type.

#### Scenario: Functions, methods, types, fields, constants and variables are declared
- **WHEN** a file declares a generic struct with an embedded field, an interface, an alias, constants, a variable, a function and two methods
- **THEN** the names are declared with their line ranges, as `Box`, `Box.Key`, `Box.Inner`, `Greeter.Greet`, `Box.Get` and the others
- **AND** a struct uses none of its field types, and the embedded field uses its type

#### Scenario: A generic receiver names the type
- **WHEN** a method has the receiver `*Box[K]` or `Box[_]`
- **THEN** it is declared as `Box.Get` or `Box.Zero`

#### Scenario: init declares nothing and its uses run on load
- **WHEN** a file declares `main` and `init`, and `init` calls `register()`
- **THEN** the file declares `main` only, and `register` is among what runs on load

#### Scenario: A package-level initialiser runs on load
- **WHEN** a file has `var table = build()`, `var _ = hook()` and a variable holding a function literal that calls `later()`
- **THEN** `build` and `hook` run on load, and `later` is a use of the variable and does not run on load

#### Scenario: A method in a file of its own exports its type
- **WHEN** `charge.go` holds only `func (s *Service) Charge() {}`
- **THEN** it exports `Service` and declares `Service.Charge`

#### Scenario: A call into the standard library or a module does not make a file run on load
- **WHEN** a package-level initialiser calls `errors.New`, `make` and a function of an imported package of the repository
- **THEN** only the last makes the file run on load

#### Scenario: Every top-level name is exported, lowercase too
- **WHEN** a file declares `Make`, `helper` and `count`
- **THEN** all three are exported by their file

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

### Requirement: Environment variables
A call of `os.Getenv`, `os.LookupEnv` or `os.Setenv` with a literal name SHALL be a site of the environment variable, a call for the first two and a serve for the last. A name that is no literal SHALL be unresolved.

#### Scenario: os.Getenv with a literal
- **WHEN** a file calls `os.Getenv("API_URL")`, `os.LookupEnv("TOKEN")`, `os.Setenv("MODE", "x")` and `os.Getenv(name)`
- **THEN** it calls `API_URL` and `TOKEN`, serves `MODE`, and the last call is unresolved at its line

### Requirement: HTTP clients
A call of `http.Get`, `Post`, `Head`, `PostForm` or `NewRequest` of `net/http`, under its alias if it has one, SHALL be a request with the method of the call. The URL SHALL be read from literals, constants and variables of the file, `+` and `fmt.Sprintf`, and an unknown part, the base of a URL first of all, SHALL read as a path.

#### Scenario: A URL with a computed base is read as a path
- **WHEN** a file calls `http.Get(base + "/api/users")`, `http.Post(base+"/api/orders", …)` and `http.Get(base)`
- **THEN** it calls `GET /api/users` and `POST /api/orders`, and the last call is unresolved

#### Scenario: An alias of net/http is read
- **WHEN** a file imports `net/http` as `nethttp` and calls `nethttp.Get("https://example.com/x")` and `nethttp.NewRequest("DELETE", "/items/1", nil)`
- **THEN** it calls `GET /x` and `DELETE /items/1`

#### Scenario: A local package named http is not read as net/http
- **WHEN** the module has a package `http` and a file imports it and calls `http.Get("/api/users")`
- **THEN** the file records no contract and no unresolved site

#### Scenario: A constant of the file fills the URL
- **WHEN** a file has the constant `users`, the variable `orders` and a local `base`, each holding a path, and passes them to `http.Get`
- **THEN** each call is a request of the path the value holds

#### Scenario: fmt.Sprintf fills the URL
- **WHEN** a file calls `http.Get(fmt.Sprintf("/api/users/%d", id))` and `http.Get(fmt.Sprintf("%s/api/users/%s/orders", "https://x.test", "a"))`
- **THEN** the first is `GET /api/users/{}` and the second is `GET /api/users/a/orders`

### Requirement: HTTP routes
A call of `HandleFunc` or `Handle`, on `net/http`, on a mux or on a router held in a field, SHALL serve its pattern. A `Handle` whose first argument is a typed value SHALL be no route. A pattern of Go 1.22 SHALL give its method and its path, and a pattern with no method SHALL serve every method.

#### Scenario: A method pattern of Go 1.22 gives the method and the path
- **WHEN** a file calls `mux.HandleFunc("GET /users/{id}", h)`, `mux.HandleFunc("/health", h)`, `http.HandleFunc("POST /orders", h)` and `http.Handle("/static/", nil)`
- **THEN** it serves `GET /users/{}`, every method of `/health`, `POST /orders` and every method of `/static`

#### Scenario: A router held in a field serves its routes
- **WHEN** a method calls `s.mux.HandleFunc("GET /a", h)` and `s.a.b.Handle("/b", h)`
- **THEN** it serves both routes

#### Scenario: A Handle whose first argument is a typed value is no route
- **WHEN** a method calls `h.inner.Handle(ctx, rec)` where `ctx` is a `context.Context` parameter, and another call passes a value of no known type
- **THEN** the first is no route and not unresolved, the other is unresolved

### Requirement: Embedded SQL
The text passed to `Query`, `QueryRow`, `Exec` or `Prepare`, and the second argument of their `Context` variants, SHALL be embedded SQL. A call whose argument is no text SHALL record nothing.

#### Scenario: A query is read as SQL
- **WHEN** a file calls `s.db.Query("SELECT id FROM orders")` and `s.db.Exec("DELETE FROM items WHERE id = 1")`
- **THEN** it embeds both statements as SQL at their lines

#### Scenario: QueryContext reads its second argument
- **WHEN** a file calls `db.QueryContext(ctx, "SELECT id FROM orders")` and `db.ExecContext(ctx, "UPDATE orders SET x = 1")`
- **THEN** it embeds both statements

#### Scenario: A call with no text in its first argument records nothing
- **WHEN** a file calls `db.Query(q)`, `db.Exec(build())` and `cache.Get(1)`
- **THEN** it embeds no SQL and has no unresolved site

### Requirement: Syntax errors do not stop the run
A file with a syntax error SHALL be recovered, with the number of errors and the line of the first. It SHALL keep its declarations, exports and imports when no top-level node is damaged, and none otherwise. A file nested deeper than the connector reads SHALL be recovered or failed, and SHALL never end the run.

#### Scenario: A file with a syntax error is recovered
- **WHEN** a file has `func Bad( {` on line 3 and another file of the package is valid
- **THEN** the first is recovered with its first error at line 3, and the other is read whole

#### Scenario: A syntax error inside a function keeps the declarations of the file
- **WHEN** a file has a top-level `var`, a function that holds `new(f(1))` and a function that calls a sibling function
- **THEN** the file is recovered and keeps its declarations, exports and imports, and the call reaches the sibling

#### Scenario: A syntax error at the top level keeps nothing
- **WHEN** a file has a stray `)` between its top-level declarations
- **THEN** the file is recovered and records no declarations and no exports, so any sibling may be reached through it
#### Scenario: A file nested too deep does not take the index down
- **WHEN** a repository holds a normal file and two files nested 100000 levels, one closed and one not
- **THEN** the run ends, the deep files are recovered or failed, and the normal file keeps its facts

### Requirement: Reads of members
A Go file SHALL record each member name it reads on a value whose type it does not know: `x.M` for a local of unknown type, `f().M`, `items[0].M` and `pkgVar.M`. A package name, a value of a type the file knows and a struct literal key SHALL record none.

#### Scenario: A file records the members it reads on a value of unknown type
- **WHEN** a file calls `x.A()` for a local of unknown type, `f().B()`, `items[0].C()` and `pkgVar.D()`, and also uses `fmt.Println`, `t.Known()` where `t` holds a type the file knows, and `T{Field: 1}`
- **THEN** the file records the reads `A`, `B`, `C` and `D`, and no other

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
