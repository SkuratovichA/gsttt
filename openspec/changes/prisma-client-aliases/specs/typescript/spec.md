## ADDED Requirements

### Requirement: Roots by callback
A chain rule MAY list callback roots, patterns of calls whose callback receives a root. The first parameter of a function or an arrow function given as the first argument of a call that a pattern matches, when it is a plain identifier, SHALL start a chain of the rule. The `prisma` pack lists `$transaction` on a receiver of one to three segments.

#### Scenario: The parameter of a transaction callback is a root whatever its name
- **WHEN** `src/untyped.ts` has `prisma.$transaction(async (client) => { await client.user.findMany(); })` with the call on line 7
- **THEN** the file calls `table user` at line 7

#### Scenario: A function expression on a nested receiver gives a root
- **WHEN** `src/nested.ts` has `this.prisma.$transaction(function (trx) { return trx.booking.create({ data }); })` with the call on line 6
- **THEN** the file calls `table booking` at line 6

#### Scenario: A callback parameter that the file binds elsewhere too is no root
- **WHEN** `src/shared.ts` has `const client = makeCache()` and a `$transaction` callback with the parameter `client`, and calls `client.user.findMany()` in both
- **THEN** the file calls no table

#### Scenario: The callback of another call is no root
- **WHEN** `src/others.ts` has `items.forEach((client) => client.user.findMany())`
- **THEN** the file calls no table

#### Scenario: A destructured callback parameter names no client
- **WHEN** `src/destructured.ts` has `prisma.$transaction(async ({ user }) => user.findMany())`
- **THEN** the file calls no table and the index is built

#### Scenario: The array form of a transaction reads as before
- **WHEN** `src/array.ts` has `prisma.$transaction([prisma.user.findMany()])` on line 6
- **THEN** the file calls `table user` at line 6
