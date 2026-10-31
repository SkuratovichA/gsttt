# Proposal: read NestJS ConfigService as environment reads

## Why

gsttt reads only member chains on `process.env` and `import.meta.env`, so a NestJS service that reads its configuration through `ConfigService` looks as if it read no environment at all. In a field test, ten of fourteen missed environment reads in one service were `this.configService.get<string>('NAME', 'default')`. Reads that go through an enum, `configService.get(Env.CLIENT_HOST)`, cannot be named and must be counted blind spots. A key such as `database.host` is a path into a loaded configuration, and upper-casing it would make a fake `env DATABASE.HOST`.

## What Changes

- A new `nestjs-config` pack, detected by `@nestjs/config`, makes `get` and `getOrThrow` on a `configService` receiver a call to `env <NAME>`.
- An `env` key that is not a variable name is skipped in every language, so a configuration path names no variable. A computed key stays a blind spot.
- `FACT_FORMAT` goes from 8 to 9: the key of cached facts does not cover the rule code, so an unchanged file would keep its old facts.

## Impact

- Capability `typescript`.
- `crates/lang-ts/packs/nestjs-config.toml`, `crates/lang-ts/src/connector.rs`, `crates/core/src/rules.rs`, `crates/core/src/engine/keys.rs`, and the fixture and tests of the TypeScript connector.
- Indexes built by an earlier version are parsed again once.
