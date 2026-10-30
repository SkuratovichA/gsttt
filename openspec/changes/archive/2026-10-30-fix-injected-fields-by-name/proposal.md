# Proposal: fields of a declared type

## Why

Asked about a method of a service, `impact` stops at the constructor of every class that receives the service through dependency injection. `constructor(private readonly invoices: InvoiceService)` names the type only in the constructor, and a method that calls `this.invoices.issue()` uses no name the file binds, so the method is not a caller. That method is the one that serves a GraphQL operation or an HTTP route, so the walk by name never crosses the contract and never reaches its clients.

## What Changes

- The TypeScript connector reads the fields of a class whose type it can name: a constructor parameter with an accessibility modifier or `readonly`, and a property with a type annotation or a `new` initializer. It reads the names a field's type stands for at its head as it reads root types for chain rules.
- A declaration that reads `this.<field>` also uses those names. A method that calls an injected service is then a caller of the service's class, and the walk by name goes on from it across the contracts it serves.

## Impact

- `crates/lang-ts`: the fields of a class and the names their types stand for, shared with the root types of chain rules; fixtures and snapshots.
- `crates/core`: the fact format one up, because the uses of a cached file change.
- Specs: `typescript` (Fields of a declared type).
