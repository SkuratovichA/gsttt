# Tasks

## 1. Core

- [x] 1.1 `Entries::First(n)`: `rows` shows `n` rows and counts the rest, `name_list` writes it as `Capped`; a test over a real answer that `First(LIST_CAP)` writes what `Capped` writes

## 2. MCP server

- [x] 2.1 `BUDGET` and `within_budget` in `ask.rs`, through which every answer text passes, with the cut line first
- [x] 2.2 The descriptions of `all` say where an answer stops; the listing snapshot; a test that every one holds `thousands(BUDGET)` and `LIST_CAP`

## 3. Documents

- [x] 3.1 `docs/architecture.md` and the skill say that an answer stops at 25,000 characters

## 4. Proof

- [x] 4.1 A test for each scenario of this change
