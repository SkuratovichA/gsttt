## MODIFIED Requirements

### Requirement: Reads of members
A file SHALL record each member name it reads on a value of a class it does not know: a name after the first of a chain, `x['m']`, a key of an object pattern, and the string a call of `spyOn`, `stub` or `spy` takes second. `this`, `super`, a known class, an object literal key and a computed key SHALL record none. `T['m']` as a type SHALL use `T.m`. A pair `on.member` comes beside these reads.

#### Scenario: A file records the members it reads on a value of unknown class
- **WHEN** a file has `x.a`, `x?.b`, `x['c']`, `const { d } = x`, `({ e } = x)`, `function f({ g }: T)`, `jest.spyOn(x, 'h')`, `<x.i />` and `a.j.k`
- **THEN** the file records the reads `a`, `b`, `c`, `d`, `e`, `g`, `h`, `i`, `j`, `k` and `spyOn`

#### Scenario: A read of a known class or of the class itself records no name
- **WHEN** a class has `this.own()`, `super.up()`, `this.repo.save()` with a typed field, and `y.known()` with `const y = new Service()`, and the file has `{ key: 1 }` and `x[k]`
- **THEN** the file records no read but the pair `repo.save`

#### Scenario: An indexed type names the member
- **WHEN** a file has `type P = Service['place']`
- **THEN** `P` uses `Service.place`

#### Scenario: A member of a field of a class below the top level is read on an unknown value
- **WHEN** a class declared inside a function has the field `service` and its method calls `this.service.update()`
- **THEN** the file records the read `update`

## ADDED Requirements

### Requirement: Members that hold a value of a class
A field, a parameter property, an interface property, a getter or a method whose written type has one head that is a class SHALL be recorded as holding a value of that class under the name `Class.member`. A primitive, a union of classes and the class itself SHALL record nothing. `A | null` holds `A`, and `Promise<T>` of a method holds `T`.

#### Scenario: A field annotated or constructed with a class holds a value of it
- **WHEN** a class has a field annotated with a class, or given `new` of a class
- **THEN** the file records the field as holding a value of the class

#### Scenario: A parameter property holds a value of its class
- **WHEN** a constructor has a parameter property annotated with a class
- **THEN** the file records the property as holding a value of the class, and a plain parameter holds nothing

#### Scenario: An interface property holds a value of its class
- **WHEN** an interface or a type alias of an object literal has a property annotated with a class
- **THEN** the file records the property as holding a value of the class

#### Scenario: A getter or a method that returns a class holds a value of it
- **WHEN** a getter or a method of a class or of an interface has a return annotation that is a class or a promise of one
- **THEN** the file records the member as holding a value of the class

#### Scenario: A primitive, a union of classes and the class itself hold nothing
- **WHEN** a member is annotated with a primitive, with a union of two classes, with a builtin, with a generic of a builtin or with its own class
- **THEN** the file records no holder for it

### Requirement: Reads of one member on another
A member read right on another member, or on what a method call gives, SHALL be recorded as the read `on.member`, seeing through `!` and `?.`. The name at the root of a chain SHALL be no `on`, and a chain on a builtin SHALL record no pair.

#### Scenario: A member read right on another is read as a pair
- **WHEN** a method has `this.ctx.logger.error()` and a function has `ctx.logger.error()` on a parameter
- **THEN** the first reads `ctx.logger` and `logger.error`, and the second reads `logger.error` and no `ctx.logger`

#### Scenario: A pair sees through a non-null assertion and optional chaining
- **WHEN** a function has `x.a!.b` and `x.c?.d`
- **THEN** the file reads the pairs `a.b` and `c.d`

#### Scenario: A member read on what a call gives is read on the method
- **WHEN** a function has `x.get().name` or `(await x.load()).title`
- **THEN** the file reads the pairs `get.name` and `load.title`

#### Scenario: A chain through an interface property reaches the file that reads it
- **WHEN** a file reads `ctx.logger.error()` for a parameter `ctx` of an interface whose property `logger` is a class
- **THEN** impact of `error` of that class lists the file, and not one that reads `ctx.logger` and calls `error` on a value nothing types
