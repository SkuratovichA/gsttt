# Proposal: a member called through a field

## Why

Asked about one method of a service, `impact` lists every method of every class that holds the service in a field, not the ones that call the method. `this.invoices.issue()` uses `InvoiceService`, the name at the head of the type of the field, and the walk by name reads a use of a class as a use of every member of it. On a NestJS service with fourteen reads of one injected field, about ten callers are listed where two call the method.

Each wrong caller also costs more than a row. The walk goes on across the contracts of the callers, so a resolver method that never calls `issue` brings the clients of the GraphQL operation or the route it serves into the answer.

## What Changes

- A declaration that reads `this.<field>.<member>`, or `this.#<field>.<member>`, uses `<Type>.<member>` for each name `<Type>` the type of the field stands for at its head, spelled as the type writes it. Any other read of the field, as a value, called itself or with a computed key, keeps using `<Type>` alone.
- The walk by name matches a use of `Type.member` with that member only, and a use of `Type` alone with every member. A declaration that uses `Type.other` is still a caller of `Type` when the class itself is asked about or reached.
- A reached member goes on to the files that import its class as the class does, through named re-exports, `export *` and a local the importer exports again. There a declaration uses it when it uses the local alone or `Local.member`. A namespace import stays coarse: any use that starts from the namespace.

## Impact

- `crates/lang-ts`: the uses of a field read for a member, their fixtures and snapshots.
- `crates/core`: the fact format one up, because the uses of a cached file change; the walk by name carries the member of a class through the files.
- Specs: `typescript` (Members read through a field of a declared type, replacing Fields of a declared type) and `queries` (Members called through a field).
