# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec

## 2. The analyzer

- [x] 2.1 `crates/lang-ts/src/analyze/makers.rs`: a variable made by `await call(…)` is a maker, and a maker keeps its first argument when it is a plain name
- [x] 2.2 `crates/lang-ts/src/extra.rs`: `ClassRoute` and `Inherit`, and the lists of them in `TsExtra`
- [x] 2.3 `crates/lang-ts/src/analyze.rs`: the routes of a class without the class decorator, the inheritance edges, the mounts of `@Module` classes and of `setGlobalPrefix`, an unread site for a computed prefix, and the hosts

## 3. The link

- [x] 3.1 `crates/lang-ts/src/link.rs`: one walk of the mounts for Express routers and NestJS classes; controller routes keyed again behind every prefix; inherited routes served in the file of the base class and of the controller

## 4. The reader

- [x] 4.1 `crates/lang-ts/src/provides.rs`: the blind spots for the global prefix and for inheritance
- [x] 4.2 `crates/lang-ts/src/project.rs`: `READER_VERSION` 2 to 3

## 5. The scenarios

- [x] 5.1 TypeScript: a test and a fixture for "A controller serves the routes of the base class it extends"
- [x] 5.2 TypeScript: a test and a fixture for "A base class that no controller extends serves nothing"
- [x] 5.3 TypeScript: a test and a fixture for "Two controllers that extend one base class serve its routes under their own prefixes"
- [x] 5.4 TypeScript: a test and a fixture for "A global prefix reaches the controllers of imported modules"
- [x] 5.5 TypeScript: a test and a fixture for "A global prefix reaches the routes a controller inherits"
- [x] 5.6 TypeScript: a test and a fixture for "Modules that import each other through forwardRef keep the prefix"
- [x] 5.7 TypeScript: a test and a fixture for "A controller that no app reaches keeps its path"
- [x] 5.8 TypeScript: a test and a fixture for "A computed global prefix is an unread site"
