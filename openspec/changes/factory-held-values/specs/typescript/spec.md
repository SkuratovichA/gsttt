# Spec Delta

## ADDED Requirements

### Requirement: Members read through what a factory gives
A static call on a class, `Service.make()`, SHALL give a value of that class, and a call of a member on a value of a known class SHALL stay on it. A top-level function, or a `const` holding a function, SHALL be an heir of the class its return type or its `new` returns, and a top-level `const x = f()` of a name of the file an heir of `f`. A lower-case name and a namespace import SHALL give no class.

#### Scenario: A member read from a constant made by a static call is a use of that member
- **WHEN** `viaConstant` has `const s = Service.make(); s.place();`
- **THEN** it uses `Service.place` and neither `Service.other` nor `Service`

#### Scenario: A member read from the result of a static call is a use of that member
- **WHEN** a function has `Service.getInstance().place()`, `(await Service.create()).place()` or `(Service.make()).place()`
- **THEN** it uses `Service.place`

#### Scenario: A chain of calls stays on the class
- **WHEN** a function has `new FixTracker(a).retain(b).remove(c)`, `const t = new FixTracker(a); t.retain(b).remove(c)` or `Service.make().with(1).place()`
- **THEN** it uses `FixTracker.retain` and `FixTracker.remove`, or `Service.with` and `Service.place`

#### Scenario: A call on a lower-case name or a namespace import names no class
- **WHEN** a function has `logger.child().info()` or `lib.make().place()` with `lib` imported as a namespace
- **THEN** the file reads `info` and `place` on a value of an unknown class and uses no member of `Service`

#### Scenario: A function that gives a class is an heir of it
- **WHEN** a file has a function whose return type is `Service`, `Promise<Service>` or `Service | null`, a function or an arrow that returns `new Service()`, and a function that returns `new Service()` in a branch
- **THEN** each is an heir of `Service`

#### Scenario: A function that gives a class only from a nested function is no heir
- **WHEN** a function returns an arrow that returns `new Service()`, or returns `new Missing()` of a class the file does not bind
- **THEN** it is an heir of nothing

#### Scenario: A top-level value made by a call is an heir of what it calls
- **WHEN** a file has `export const made = typed()` with `typed` a function of the file, `export const waited = await promised()` and `export const fromStatic = Service.make()`
- **THEN** `made` is an heir of `typed`, `waited` of `promised` and `fromStatic` of `Service`
- **AND** a call of a name the file does not bind, and of a member of a namespace import, gives no heir

#### Scenario: A function required with CommonJS is a use of its class
- **WHEN** a file has `const { makeService } = require('./service'); makeService().place();` and `const lib = require('./service'); lib.makeService().place();`
- **THEN** the declarations use `makeService` and `lib.makeService`, and the file reads `place`
