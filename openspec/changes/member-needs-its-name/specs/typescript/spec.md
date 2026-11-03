# Spec Delta

## ADDED Requirements

### Requirement: Reads of members
A file SHALL record each member name it reads on a value of a class it does not know: a name after the first of a chain, `x['m']`, a key of an object pattern, and the string a call of `spyOn`, `stub` or `spy` takes second. `this`, `super`, a known class, an object literal key and a computed key SHALL record none. `T['m']` as a type SHALL use `T.m`.

#### Scenario: A file records the members it reads on a value of unknown class
- **WHEN** a file has `x.a`, `x?.b`, `x['c']`, `const { d } = x`, `({ e } = x)`, `function f({ g }: T)`, `jest.spyOn(x, 'h')`, `<x.i />` and `a.j.k`
- **THEN** the file records the reads `a`, `b`, `c`, `d`, `e`, `g`, `h`, `i`, `j`, `k` and `spyOn`

#### Scenario: A read of a known class or of the class itself records no name
- **WHEN** a class has `this.own()`, `super.up()`, `this.repo.save()` with a typed field, and `y.known()` with `const y = new Service()`, and the file has `{ key: 1 }` and `x[k]`
- **THEN** the file records no read

#### Scenario: An indexed type names the member
- **WHEN** a file has `type P = Service['place']`
- **THEN** `P` uses `Service.place`

#### Scenario: A member of a field of a class below the top level is read on an unknown value
- **WHEN** a class declared inside a function has the field `service` and its method calls `this.service.update()`
- **THEN** the file records the read `update`
