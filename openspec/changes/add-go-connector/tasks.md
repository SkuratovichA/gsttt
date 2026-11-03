# Tasks

## 1. Reading files

- [x] 1.1 `Connector` for `go`: detection, one project per `go.mod`, vendored code, test data and hidden files left out, test files tagged
- [x] 1.2 Parsing with `tree-sitter-go`: imports, declarations, uses, `init`, package-level initialisers, recovered and too-deep files

## 2. Resolution

- [x] 2.1 Imports against the module path, nested modules, required modules and the standard library
- [x] 2.2 Files of one package reach each other by the names they read

## 3. Packs

- [x] 3.1 Packs: `env`, `net-http`, `database-sql`, with values read from literals, constants of the file and `fmt.Sprintf`
- [x] 3.2 `provides` with the gaps of each project

## 4. Wiring

- [x] 4.1 Register `go` after `rust` in the binary, its tests and the docs

## 5. Proof

- [x] 5.1 A test annotated `#[scenario("go", …)]` for every scenario of the `go` spec, with reviewed `insta` snapshots
- [x] 5.2 End-to-end tests for the `queries` scenarios of Go
- [x] 5.3 gsttt gates pass: `cargo fmt`, `taplo`, `clippy`, `nextest`, `cargo xtask spec`
