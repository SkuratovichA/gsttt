# Tasks

## 1. Imports

- [x] 1.1 `visit_ts_import_type` reads `typeof import('x')` and `import('x').T` as a type-only static import at the line of the specifier, naming the first segment of the qualifier, and walks into the types nested in it
- [x] 1.2 The first argument of the mock, unmock and actual-module calls of Vitest and Jest is a dynamic import with no names through `load`, when it is a string or a template with no expression, and a computed one is neither an import nor an unread site
- [x] 1.3 `resolve` drops the query of a specifier before resolving it, and the fact keeps the specifier as written
- [ ] 1.4 The connector version is 2
- [x] 1.5 A test annotated `#[scenario("typescript", …)]` for every scenario of the requirements Imports in types, Mocked modules and Query suffixes

## 2. Prisma tables

- [ ] 2.1 Tables through aliases of this, typed roots and imports of model types
