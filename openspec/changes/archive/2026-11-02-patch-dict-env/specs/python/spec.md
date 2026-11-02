## MODIFIED Requirements

### Requirement: Environment writes
With the `env` pack, `os.environ["<NAME>"] = …`, an augmented assignment to it, `del os.environ["<NAME>"]`, `os.putenv`, `os.unsetenv`, `monkeypatch.setenv` and `monkeypatch.delenv` with a literal name SHALL serve `env <NAME>` and not call it. `os.environ.setdefault` and `os.environ.pop` SHALL call and serve it. A name that is not a literal SHALL leave one unread site per call.

#### Scenario: A write of an environment variable serves it
- **WHEN** a file has `os.environ["A"] = "x"`, `os.environ["B"] += ":y"`, `del os.environ["C"]`, `os.putenv("D", "z")`, `os.unsetenv("E")`, and a function that calls `monkeypatch.setenv("F", "1")` and `monkeypatch.delenv("G")`
- **THEN** it serves `env A` to `env G`, calls no `env` and leaves no unread site

#### Scenario: Setting a default or popping a variable calls and serves it
- **WHEN** a file has `os.environ.setdefault("PORT", "8000")` and `os.environ.pop("TOKEN", None)`
- **THEN** it calls and serves `env PORT` and `env TOKEN`

#### Scenario: A computed name in a write is one unread site
- **WHEN** a file has `os.environ[NAME] = "x"`, `os.environ.setdefault(NAME, "y")` and `os.environ.pop(NAME, None)`
- **THEN** it names no `env` and leaves one unread site for each of the three lines


## ADDED Requirements

### Requirement: Environment writes through a mapping
A call that writes keys into a mapping the env pack roots SHALL serve each literal key it sets: `patch.dict`, as a call, in a `with` or as a decorator, `monkeypatch.setitem`, `monkeypatch.delitem`, `.update` and `|=`; the mapping may be named by a string. A computed key, a `**` item and values that are no dict literal SHALL leave one unread site per call. A write into another mapping, and one with no keys, SHALL name nothing.

#### Scenario: patch.dict serves the keys and keywords it sets
- **WHEN** a file has `patch.dict` of `os.environ` with the dict `{"A": "1", "B": "2"}` in a `with`, with `values={"C": "3"}` and `D="4"`, with `in_dict=os.environ` and `E="5"`, and with `{"F": "6"}` and `clear=True`, and a call `patch.dict(os.environ, clear=True)`
- **THEN** it serves `env A` to `env F`, calls no `env` and leaves no unread site

#### Scenario: patch.dict as a decorator serves its keys
- **WHEN** a file has `@patch.dict(os.environ, {"A": "1"})` on a function and `@patch.dict(os.environ, {"B": "2"}, C="3")` on a class
- **THEN** it serves `env A`, `env B` and `env C` on the lines of the decorators and leaves no unread site

#### Scenario: A mapping named by a string is written
- **WHEN** a file has `patch.dict("os.environ", {"A": "1"})` and `patch.dict("settings", {"B": "2"})`
- **THEN** it serves `env A` only and leaves no unread site

#### Scenario: A mapping write without values names nothing
- **WHEN** a file has `patch.dict(os.environ)`, `patch.dict(os.environ, clear=True)`, `os.environ.update()` and `print(os.environ)`
- **THEN** it names no `env` and leaves no unread site

#### Scenario: A computed key in a mapping write is one unread site per call
- **WHEN** a file has `patch.dict(os.environ, {"A": "1", NAME: "2", **BASE, OTHER: "3"})`, `patch.dict(os.environ, {"B": "1"}, **extra)`, `patch.dict(os.environ, values)` and `patch.dict(os.environ, dict(C="1"))`
- **THEN** it serves `env A` and `env B`, and each of the four calls is one unread site

#### Scenario: monkeypatch.setitem and delitem write one key
- **WHEN** a file has `monkeypatch.setitem(os.environ, "A", "1")` and `monkeypatch.delitem(os.environ, "B")`, and the same calls with the key `NAME`
- **THEN** it serves `env A` and `env B`, and each call with `NAME` is one unread site

#### Scenario: update and |= serve the keys they set
- **WHEN** a file has `os.environ.update({"A": "1"}, B="2")`, `environ |= {"C": "3"}` after `from os import environ`, `os.environ |= {"D": "4", NAME: "5"}`, `os.environ.update(extra)` and `os.environ.update({"E": "6"}, **extra)`
- **THEN** it serves `env A` to `env E`, and the last three calls are one unread site each

#### Scenario: A write into another mapping names nothing
- **WHEN** a file has `patch.dict(settings, {"A": "1", NAME: "2"}, B="3")`, `d.update({"C": 1}, **extra)`, `d |= {"D": 1}`, `monkeypatch.setitem(config, "E", 1)` and `monkeypatch.setitem(config, NAME, 1)`
- **THEN** it names no `env` and leaves no unread site

#### Scenario: A read inside the values of a mapping write is still a read
- **WHEN** a file has `patch.dict(os.environ, {"A": os.environ["B"]})`
- **THEN** it serves `env A` and calls `env B`
