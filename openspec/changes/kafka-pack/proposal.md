# Proposal: A Kafka pack that names the producer by its declared type

## Why

A project pack in a field repository read the Kafka producer with `callee = ["*.emitMessage", "*.*.emitMessage", "*.emit", "*.*.emit"]`, so `eventEmitter.emit("change", token)` became a queue call: `emit` says nothing without the receiver. A built-in pack has to name the producer by the type its receiver is declared with, and there is none today, so queue contracts are found only with a project pack.

## What Changes

- Core: a call rule has an optional `receiver_types`, mirroring `root_types` of a chain rule. The pack schema gains the field.
- TypeScript: the walk that finds names declared with a root type (`analyze/roots.rs`) watches the receiver types of the call rules as well, and a call rule that lists receiver types reads a call only on a receiver declared with one of them. The decision is made at parse, so the link phase is unchanged: `RuleRef::Call` is only used there to look a rule up, after the parse has deferred the call.
- Python and Rust: a call rule with `receiver_types` reads nothing, because those connectors know no declared type and reading every receiver would make false edges.
- A `kafka` pack: `@EventPattern` and `@MessagePattern` serve a queue, `emit`, `send` and `emitMessage` on a `ClientKafka`, `ClientProxy`, `KafkaProducerService` or `Producer` call it, and `subscribe` on a `Consumer` serves it. It is detected by `kafkajs`, the peer dependency of the NestJS Kafka transport, so a project on another transport has none of it.
- The `queue` contracts state their blind spots: a producer or consumer held in a name with no declared type, and topics given as an array.
- Cache: `module_state` hashes the serialised packs, so the files the new pack applies to are parsed again, and `receiver_types` is left out of the serialised rule when empty, so no other rule changes its hash. The roots walk is skipped for a file whose packs list no type, and changes no fact for one that has no such rule. No `FACT_FORMAT`, `READER_VERSION` or `LAYOUT` bump.

## Impact

- Capabilities `typescript`, `python` and `rust`; `crates/core/src/rules.rs`, `crates/lang-ts` (`analyze/roots.rs`, `analyze.rs`, `provides.rs`, the pack), `crates/lang-python/src/parse.rs`, `crates/lang-rust/src/sites.rs`, the README.
- Limits: `const producer = kafka.producer()` and topics given as `{ topics: [...] }` are not read; the second comes with a change that reads every element of an array.
