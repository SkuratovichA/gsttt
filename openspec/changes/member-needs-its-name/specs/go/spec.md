# Spec Delta

## ADDED Requirements

### Requirement: Reads of members
A Go file SHALL record each member name it reads on a value whose type it does not know: `x.M` for a local of unknown type, `f().M`, `items[0].M` and `pkgVar.M`. A package name, a value of a type the file knows and a struct literal key SHALL record none.

#### Scenario: A file records the members it reads on a value of unknown type
- **WHEN** a file calls `x.A()` for a local of unknown type, `f().B()`, `items[0].C()` and `pkgVar.D()`, and also uses `fmt.Println`, `t.Known()` where `t` holds a type the file knows, and `T{Field: 1}`
- **THEN** the file records the reads `A`, `B`, `C` and `D`, and no other
