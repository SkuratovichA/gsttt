# Spec Delta

## MODIFIED Requirements

### Requirement: What a declaration uses
A declaration SHALL record, once each, the names it refers to that the file declares or binds by import: identifiers, JSX tags, type references and `typeof`, `Class.member` for `this.member` inside the class, `C.s` alone for `C.s`, and `Class.` for a type alone. An export specifier SHALL NOT count, nor a use of the declaration itself. Shadowing SHALL be ignored.

#### Scenario: References, tags, types and this are uses
- **WHEN** a file imports `useX`, `Props` and `Widget`, declares `helper`, `function View(p: Props) { const x = useX(); return <Widget value={helper(x)} /> }`, `class C { a() { this.b() } b() {} static s() {} }` and `function d() { return C.s() }`
- **THEN** `View` uses `Props.`, `Widget`, `helper` and `useX`, `C.a` uses `C.b`, and `d` uses `C.s` alone, matching the reviewed snapshot

## ADDED Requirements

### Requirement: Members read through a value of a known class
A name bound below the top level of a file by `const` to `new` of a class, or declared by a `const` or a parameter with a plain class type, SHALL hold that class: a member read from it SHALL be a use of `Class.member` only, the name read as a value a use of `Class`, and its type a use of `Class.`. `new` SHALL use `Class.constructor` when its value goes no further. `Class.member` on an imported name or on a class of the file with that member SHALL be a use of that member only.

#### Scenario: A member read from a new instance is a use of that member
- **WHEN** `refund` has `new OrderService().cancel(id)`
- **THEN** `refund` uses `OrderService.cancel` and `OrderService.constructor` and not `OrderService`

#### Scenario: A member read from a constant made by new is a use of that member
- **WHEN** `refund` has `const service = new OrderService(); service.cancel(id);`
- **THEN** `refund` uses `OrderService.cancel` and `OrderService.constructor` and not `OrderService`

#### Scenario: A member read from a parameter of a class type is a use of that member
- **WHEN** `refund` is `function refund(service: OrderService) { service.cancel(id); }`
- **THEN** `refund` uses `OrderService.cancel` and `OrderService.` and not `OrderService`

#### Scenario: A value of a class read as a value is a use of the class
- **WHEN** `pass` is `function pass(service: OrderService) { run(service); }` and `keep` has `const service = new OrderService(); run(service);`
- **THEN** `pass` and `keep` use `OrderService`

#### Scenario: A static member of an imported class is a use of that member
- **WHEN** `create` has `OrderService.create()`
- **THEN** `create` uses `OrderService.create` and not `OrderService`

#### Scenario: A static member of a class of the file is a use of that member
- **WHEN** the file declares `class Runner` with `static start()` and `static stop()`, and `begin` has `Runner.start()`
- **THEN** `begin` uses `Runner.start` and not `Runner`

#### Scenario: A type with arguments holds no class
- **WHEN** `load` is `function load(repo: Repository<User>) { repo.find(); }`, with `Repository` and `User` imported
- **THEN** `load` uses `Repository` and `User`

#### Scenario: A name bound twice in a scope holds no class
- **WHEN** a function has `const a = new OrderService(); const a = new OtherService(); a.cancel();`
- **THEN** it uses neither `OrderService.cancel` nor `OtherService.cancel`
