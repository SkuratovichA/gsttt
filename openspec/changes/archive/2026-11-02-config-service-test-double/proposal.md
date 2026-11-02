# Proposal: A service a test provides itself reads nothing

## Why

`configService.get(Env.X)` in a test that provides its own `ConfigService` never reads the environment, so listing the test as a reader of the variable is a false edge. A field test on a public project found two, each in a spec file with `provide: ConfigService, useValue` in the same file.

## What Changes

- Core: a call rule has an optional `test_double`, the injection token a test replaces with a stand-in. The pack schema gains the field.
- TypeScript: a pre-walk finds, in a test file, the tokens it provides a stand-in for, by an object with `provide` and one of `useValue`, `useFactory`, `useClass` or `useExisting`, or by a call of the method `overrideProvider`. A rule whose `test_double` is among them reads nothing in that file: no contract, no deferred site and no unread site, wherever the read stands in the file. The connector names no framework.
- The `nestjs-config` pack sets `test_double = "ConfigService"`.
- Cache: `module_state` hashes the serialised packs, so the files that use the pack are parsed again; no `FACT_FORMAT`, `READER_VERSION` or `LAYOUT` bump.

## Impact

- Capability `typescript`; `crates/core/src/rules.rs`, `crates/lang-ts` (`analyze/doubles.rs`, `analyze.rs`, `source.rs`, `project.rs`, the pack), the README.
- Limits: a stand-in is seen only in the file that reads it, and only when provided by `provide` or `overrideProvider`, so a `const configService = { get: jest.fn() }`, a provider in a shared helper and a renamed import keep the read. A file that is no test keeps its reads.
