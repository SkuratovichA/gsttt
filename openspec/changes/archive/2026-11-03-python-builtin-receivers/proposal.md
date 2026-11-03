# Proposal: Count no member read on a value the file shows is a builtin

## Why

Asked about a method, gsttt lists every file that reads a member of that name on a value whose type it cannot tell. Many of those values are builtins the file can see: a file from `open`, the keys of `vars(x)`, a list bound from a field, a match of `re`. A method such as `Text.append` of rich then lists every file that appends to a list, which makes the answer a grep.

## What Changes

- Python: `open`, `vars`, `dir`, `globals` and `locals` are builtin calls, as `list` and `sorted` are, and a name of the file that rebinds one of them keeps the read. `Path` and `PurePath` from `pathlib` are builtin types, and so are `path.joinpath(...)`, `with_name`, `with_suffix`, `resolve`, `absolute`, `expanduser` and `path / "name"` of builtin values.
- Python: a name that `with` binds to a call of a builtin, or to `open` called on a builtin value such as a `Path`, holds a builtin. A class in a `with` says nothing, because its `__enter__` may return anything.
- Python: a local bound from a builtin local, or from a field of `self` or of a typed parameter that holds a builtin, holds a builtin.
- Python: `re.compile`, `re.match`, `re.search` and `re.fullmatch` give a builtin, and so does the variable of a `for` over `re.finditer`, `re.split` or `re.findall`, as long as the file imports `re` and does not bind the name again at its top level. A `for` over any other value stays unknown, because the element of a list of `Foo` is a `Foo`.
- Limits: an element of a builtin collection, as in `for line in text.splitlines()`, stays unknown, because a builtin does not say which builtin it is. A chain through a subscript of a list of objects stays a read.
- The receiver of a method is typed as an instance of its class where a local is bound from it, so `b = self` holds an instance of the class, as `self` does.
- Versions: the `version` of the Python connector's `Meta` goes from 5 to 6, because the reads it stores change. `FACT_FORMAT` and `LAYOUT` stay.

## Impact

- Capability `python`.
- `BUILTIN_CALLS`, `BUILTIN_TYPES`, `Table::with_type`, `Table::iteration_type` and `Table::value_type` in `crates/lang-python/src/symbols/table.rs`, the `with` and `for` of the recorder in `crates/lang-python/src/symbols.rs` and of `Scan` in `crates/lang-python/src/symbols/locals.rs`.
