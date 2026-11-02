# Proposal: Read the environment variables a mapping call writes

## Why

A Python test setup sets environment variables through a mapping call as often as through a subscript: `patch.dict(os.environ, {"API_URL": "x"})`, `monkeypatch.setitem(os.environ, "API_URL", "x")`, `os.environ.update(…)` and `os.environ |= …`. The `env` pack read none of them, and the provides named them a blind spot, so a variable such a test sets had no server in the graph and the question "what serves env X" got a wrong answer.

## What Changes

- Python: a call that writes keys into a mapping that a chain rule roots is read through that rule's `written` role, with no code that knows about `env`. The calls are `patch.dict`, `mock.patch.dict`, `unittest.mock.patch.dict` and `mocker.patch.dict` (under every spelling an import gives them) as a call, in a `with` and as a decorator of a function or a class, `monkeypatch.setitem` and `monkeypatch.delitem`, `<mapping>.update(…)`, and `<mapping> |= …`. The mapping is a name or, for `patch.dict`, a string such as `"os.environ"`.
- Each string key of a dict literal and each keyword the call does not use for its own arguments serves the variable. A key that is computed, a `**` item, a `**` keyword and values that are no dict literal are one unread site for the call. A call with no values and no keywords names nothing. A write into a mapping no chain rule roots names nothing.
- The subscript and the mapping calls share one routine that applies the chain rules, `Collector::touch`.
- The blind spot for these forms is removed from the provides, and the comment of the `env` pack says what is read.
- Versions: the Python connector goes from 3 to 4, because it hands the engine facts it did not read before, and the version is part of the key of cached facts, so an unchanged file would keep the old ones.

## Impact

- Capability `python`.
- `crates/lang-python/src/parse.rs`, `provides.rs`, `lib.rs` and `packs/env.toml`.
- A tree with none of these calls gives the same index as before. A tree with one gives more `serves env` facts, and an unread site where a key cannot be read.
