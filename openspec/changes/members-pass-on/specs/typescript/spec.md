# Spec Delta

## ADDED Requirements

### Requirement: Heirs
The connector SHALL record, for a name, the class or interface whose members it takes: the head of the `extends` of a class, each head of the `extends` of an interface, the class a top-level `const` holds, and `default` for `export default new Service()`. A head SHALL be the dotted name without type arguments. `implements`, a call such as `mixin(Base)`, `let`, `var` and a value below the top level SHALL record nothing.

#### Scenario: A class records what it extends
- **WHEN** a file has `class Plain extends Base<string>`, `class Qualified extends lib.Base`, `class Mixed extends mixin(Base)` and `class Promised implements Alone`
- **THEN** it records `Plain` of `Base` and `Qualified` of `lib.Base`
- **AND** it records no heir for `Mixed` or `Promised`

#### Scenario: An interface records what it extends
- **WHEN** a file has `interface Wide extends Base, lib.Shape` and `export default interface Named extends Base`
- **THEN** it records `Wide` of `Base`, `Wide` of `lib.Shape` and `Named` of `Base`

#### Scenario: A top-level value records the class it holds
- **WHEN** a file has `export const made = new Service()`, `const typed: Service = make()`, `const generic = new Repository<string>()`, `let later = new Service()`, `var oldest = new Service()` and a `const` in a function, and another has `export default new Service()`
- **THEN** it records `made` and `typed` of `Service` and `generic` of `Repository`
- **AND** it records nothing for the `let`, the `var` and the `const` in a function
- **AND** the other file records `default` of `Service`

### Requirement: Members of the head
In a class with a head, `super.member` SHALL be a use of `Head.member` even when the class declares that member, `this.member` of a member the class does not declare a use of `Head.member`, and `super(...)` a use of `Head.constructor`. A member read from a name that has a head SHALL be used by that name. A class with no head SHALL record none of these.

#### Scenario: `super` and an inherited `this` name the member of the head
- **WHEN** `class Sub extends Base` has `place`, which calls `super.place()`, `this.place()` and `this.own()` for its own `own`, a constructor that calls `super(x)`, and `class Headless` calls `this.go()` and `super.go()`
- **THEN** `Sub.place` uses `Base.place` and `Sub.own`, and `Sub.constructor` uses `Base.constructor`
- **AND** `Headless.run` uses nothing

#### Scenario: A member read from a name with a head is used by its name
- **WHEN** a file has `export const orders = new Service()` and a function that calls `orders.place()`, and a function that calls `Sub.make()` for a class `Sub` that extends `Base`
- **THEN** the first function uses `orders.place`
- **AND** the second uses `Sub.make`
