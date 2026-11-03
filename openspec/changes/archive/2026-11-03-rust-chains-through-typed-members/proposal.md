# Proposal: Record the Rust members that hold a value and the reads through them

## Why

Asked about `StyleComponents.changes`, gsttt misses a file that writes `self.config.style_components.changes()`, because the file never names `StyleComponents`. The core already follows such a chain through a member that holds a value (`holds`) and the reads of one member right on another (`a.b` in `reads`). Python fills both and Rust does not.

## What Changes

- A named field, or a method taking `self`, whose written type is a class is recorded in `holds` as `Class.member` with that class. `Option<T>` and `Result<T, E>` are looked into for the class, because `arg.get_num_args().expect("x").min_values()` reads through the `T`.
- `Self`, the class itself, a class of the standard library and an associated function without `self` record nothing.
- A member read right on a field or on what a method gives is recorded in `reads` as `on.member`, seeing through parentheses, references, `?`, `.await` and the calls that only hand on the value they hold (`unwrap`, `expect`, `as_ref`, `as_mut`, `as_deref`, `clone`). The pair is recorded whether the class of the receiver is known or not, and on a value of the standard library too, because no type is inferred.
- Limits: a chain broken by a local or by a closure parameter is not followed, because its type is not written next to the use; no type inference is added.
- Versions: the `version` of the Rust connector goes from 6 to 7, because the connector reads more. `FACT_FORMAT` and `LAYOUT` stay, because the stored shape does not change.

## Impact

- Capability `rust`: two requirements are added.
- `crates/lang-rust/src/symbols.rs`, `crates/lang-rust/src/symbols/table.rs`, `crates/lang-rust/src/symbols/receivers.rs` and `crates/lang-rust/src/lib.rs`.
- A Rust file that reads a member through a field or a method of another class is listed for the member it reads; a file that reads the same name on a value it cannot link to that chain is not.
