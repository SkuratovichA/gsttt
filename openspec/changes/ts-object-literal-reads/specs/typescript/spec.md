## ADDED Requirements

### Requirement: A key of an object literal the file wrote is no read
A member chain on a name that the file binds once, by `const`, to an object literal SHALL record no read for the links that are keys of that literal, nested where a key holds a literal, and SHALL record the links after them as before. A computed key SHALL add no key, a spread SHALL hide what the keys before it hold, and a cast or a name bound otherwise SHALL give no shape.

#### Scenario: A key of an object literal the file wrote is no read
- **WHEN** a file has `const m = { resource: { findMany: f } }; m.resource.findMany.mockReturnValueOnce([])`, or `const o = { place() {} }; o.place()`
- **THEN** it reads `mockReturnValueOnce` and neither `resource` nor `findMany`, and the second reads nothing

#### Scenario: A member read below a key that holds another value is a read
- **WHEN** a file has `const d = { svc: make() }; d.svc.place()`, or `const o = { a: { place() {} } }; o.a.place(); o.b.other()`
- **THEN** it reads `place`, or `b` and `other`, and neither `svc` nor `a`

#### Scenario: A spread, a computed key, a cast or a destructuring hides no read
- **WHEN** a file has `const o = { ...base, x: 1 }; o.x; o.place()`, `const o = { ['place']: 1 }; o.place`, `const a = {} as Service; a.place()`, or `const { resource } = literal; resource.findMany()`
- **THEN** it reads `place` or `findMany`, and `o.x` reads nothing

#### Scenario: A name bound again gives no shape
- **WHEN** a file binds the name again with `let o = {…}; o = make()`, a parameter `o` or an inner `const o = make()`, and reads `o.place()`
- **THEN** it reads `place`
