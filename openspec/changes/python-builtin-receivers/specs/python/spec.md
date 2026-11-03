# Spec Delta

## ADDED Requirements

### Requirement: Builtin values the file shows
A member read on a value the file shows is a builtin SHALL be no read: a call of `open`, `vars`, `dir`, `globals`, `locals` or a builtin type, the name of a `with` over such a call, a local bound from a builtin local or field, and a match of `re`. A builtin name the file binds itself SHALL keep the read, and so SHALL a class in a `with`.

#### Scenario: A name a with binds to a builtin call is no receiver of a member
- **WHEN** a function reads `read` on the name of `with open(p) as fh`, and `write` on the name of `with tmp.open("w") as fh` where `tmp` is annotated `Path` from `pathlib`, also when the path is `tmp.joinpath("a")` or `tmp / "b"`
- **THEN** the file records no read
- **AND** `read` on the name of `with Foo() as fh`, `open` and `write` on the name of `with tmp.open("w") as fh` with an unannotated `tmp`, and `read` on a name bound again by `fh = make()` after the `with`, are reads

#### Scenario: A builtin the file binds itself is no builtin call
- **WHEN** a file defines `def open(p)` or imports `open` from a module of the project, and reads `read` on the name of `with open(p) as fh`, or assigns `vars = make` and reads `keys` on `vars(x)`
- **THEN** the file records the reads `read` and `keys`

#### Scenario: A member read on a call of a builtin is no read
- **WHEN** a function reads a member on `vars(x)`, `dir(x)`, `globals()`, `locals()`, `str(x)` and `sorted(x)`, and on the variable of a `for` over `vars(x).keys()`
- **THEN** the file records only the read on the loop variable

#### Scenario: A local bound from a builtin local or field is no receiver of a member
- **WHEN** a function assigns `x = [a]` and reads `append` on `x`, on `ys = x`, and on `append = x.append`, and a method reads `append` on `b = self._items` where the class assigns `_items` a list or annotates it `List[str]`
- **THEN** the file records no read
- **AND** `append` on `x` assigned `make()` afterwards, and on `b = self._items` where the class assigns `_items` a parameter with no annotation, are reads

#### Scenario: A match of re is no receiver of a member
- **WHEN** a file imports `re` and a function reads `span` on the result of `re.compile`, `re.match`, `re.search` and `re.fullmatch` and on the variable of a `for` over `re.finditer`, `re.split` or `re.findall`
- **THEN** the file records no read
- **AND** `span` on the variable of a `for` over an unknown value, and on the result of `re.search` where the file assigns `re = make()` at its top level, is a read
