## ADDED Requirements

### Requirement: Environment writes
With the `env` pack, `os.environ["<NAME>"] = …`, an augmented assignment to it, `del os.environ["<NAME>"]`, `os.putenv`, `os.unsetenv`, `monkeypatch.setenv` and `monkeypatch.delenv` with a literal name SHALL serve `env <NAME>` and not call it. `os.environ.setdefault` and `os.environ.pop` SHALL call and serve it. `os.environ.update(…)` and `os.environ |= …` SHALL not be read, which the provides name. A name that is not a literal SHALL leave one unread site per call.

#### Scenario: A write of an environment variable serves it
- **WHEN** a file has `os.environ["A"] = "x"`, `os.environ["B"] += ":y"`, `del os.environ["C"]`, `os.putenv("D", "z")`, `os.unsetenv("E")`, and a function that calls `monkeypatch.setenv("F", "1")` and `monkeypatch.delenv("G")`
- **THEN** it serves `env A` to `env G`, calls no `env` and leaves no unread site

#### Scenario: Setting a default or popping a variable calls and serves it
- **WHEN** a file has `os.environ.setdefault("PORT", "8000")` and `os.environ.pop("TOKEN", None)`
- **THEN** it calls and serves `env PORT` and `env TOKEN`

#### Scenario: A computed name in a write is one unread site
- **WHEN** a file has `os.environ[NAME] = "x"`, `os.environ.setdefault(NAME, "y")` and `os.environ.pop(NAME, None)`
- **THEN** it names no `env` and leaves one unread site for each of the three lines
