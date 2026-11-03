# Spec Delta

## MODIFIED Requirements

### Requirement: A star barrel names nothing
Asked about a name, a file that passes it on by `export *` SHALL get no row, because nothing in it names the name; the walk SHALL go through it with no hop and reach its importers as before. A file that re-exports a class, `export * as ns` or hands a class on by `module.exports.A = A` SHALL get none either for a member of the class or a name of the namespace. A file that names what it passes on keeps its row, and so does a star barrel that lists the name among its own exports.

#### Scenario: A star barrel gets no row when a name is asked about
- **WHEN** `index.ts` has `export * from './a'`, `page.ts` imports `helper` from `index.ts` and uses it in `open`, and impact is asked about `helper` in `a.ts`
- **THEN** impact lists `page.ts` at distance 1 with the reason `imports helper from index.ts`, and no other file

#### Scenario: A chain of star barrels gets no row
- **WHEN** `index.ts` has `export * from './a'`, `outer.ts` has `export * from './index'`, a test imports `helper` from `outer.ts`, and impact is asked about `helper` in `a.ts`
- **THEN** impact lists the test at distance 1, and neither barrel

#### Scenario: A named re-export keeps its row when a name is asked about
- **WHEN** `index.ts` has `export { helper } from './a'`, `page.ts` imports `helper` from `index.ts` and uses it in `open`, and impact is asked about `helper` in `a.ts`
- **THEN** impact lists `index.ts` at distance 1 with the reason `re-exports helper from a.ts`, and `page.ts` at distance 1

#### Scenario: A re-export of a class gets no row for a member of it
- **WHEN** `index.ts` has `export { Service } from './service'`, `page.ts` imports `Service` from `index.ts` and its `go` uses `Service.place`, and impact is asked about `Service.place` in `service.ts`
- **THEN** impact lists `page.ts` and not `index.ts`
- **AND** asked about `Service`, impact lists both

#### Scenario: A namespace re-export gets no row for a name it holds
- **WHEN** `index.ts` has `export * as lib from './a'`, `page.ts` imports `lib` from `index.ts` and its `go` uses `lib.helper`, and impact is asked about `helper` in `a.ts`
- **THEN** impact lists `page.ts` and not `index.ts`

#### Scenario: A file that hands a class on gets no row for a member of it
- **WHEN** `mid.js` exports a class of `base.js` under another name by `module.exports.Agent = Base`, `page.js` imports it and calls `Agent.destroy`, and impact is asked about `Base.destroy`
- **THEN** impact lists `page.js` and not `mid.js`
- **AND** asked about `Base`, impact lists both

#### Scenario: A star barrel that lists the name among its own exports keeps its row
- **WHEN** `index.ts` passes on `./a` by `export *` and lists `helper` among its own exports, `page.ts` imports `helper` from `index.ts` and uses it in `open`, and impact is asked about `helper` and about `other` in `a.ts`
- **THEN** about `helper` impact lists `index.ts` at distance 1 with the reason `re-exports helper from a.ts`, and `page.ts` at distance 1 with the reason `imports helper from index.ts` and the caller `open`
- **AND** about `other` it lists neither


## REMOVED Requirements

### Requirement: Names in files without declarations
**Reason**: Python records declarations now, so its scenario names what is no longer true, and a scenario of a modified requirement cannot be renamed or dropped.
**Migration**: "Names in files that record no declarations" says the same of Rust, the one language left; the scenario "An importer without declarations is followed whole" moves there unchanged, and "A Rust file asked about by name is followed whole" replaces "A Python file asked about by name is followed whole".

## ADDED Requirements

### Requirement: Names in files that record no declarations
A file whose connector records no declarations SHALL be followed whole: asked about a name in such a file, `impact` SHALL walk from it as it does without names and say so in a note, and an importer without declarations, like an edge into a file whose exports are unknown, SHALL be followed whole from there. Rust records no declarations yet.

#### Scenario: A Rust file asked about by name is followed whole
- **WHEN** `user.rs` imports `parse` from `lib.rs`, `app.rs` imports `user.rs`, and impact is asked about `parse` in `lib.rs`
- **THEN** impact lists `user.rs` at distance 1 and not `app.rs`
- **AND** a note says that `lib.rs` is followed whole because its connector records no declarations

#### Scenario: An importer without declarations is followed whole
- **WHEN** `b.ts` imports `foo` from `a.ts` and was read past syntax errors, `c.ts` imports `bar` from `b.ts` and uses it in `main`, and impact is asked about `foo` in `a.ts` with depth 2
- **THEN** impact lists `b.ts` at distance 1 without callers and `c.ts` at distance 2 with the caller `main`

### Requirement: A name that is a module
An import flagged as one whose names may be modules, as Python's `from pkg import sub`, SHALL bind a name to the module itself when the file the walk reaches is that module of the package, so that `sub.foo` in an importer is a caller of `foo` in `pkg/sub.py`. It SHALL bind the name to the value that module exports under that name when the import names the module itself, as `from app.logger import logger` does. An import without the flag SHALL bind a name to no module.

#### Scenario: A name a package hands out is the module when the file reached is that module
- **WHEN** `pkg/sub.py` declares `foo` and `bar`, `pkg/__init__.py` declares `sub`, `b.py` takes `sub` from `pkg` and uses `sub.foo`, and `c.py` takes it and uses `sub.bar`, and impact is asked about `foo` and `bar` in `pkg/sub.py` and about `sub` in `pkg/__init__.py`
- **THEN** about `foo` impact lists `b.py` and not `c.py`, and about `bar` it lists `c.py` and not `b.py`
- **AND** about `sub` in the `__init__` it lists both

#### Scenario: A name a package hands out is no module unless the import says it may be
- **WHEN** the same import of `sub` carries no flag and impact is asked about `foo` in `pkg/sub.py`
- **THEN** impact lists no file

#### Scenario: A name a file exports itself is that name and not the module of the same stem
- **WHEN** `app/logger.py` declares `logger`, `a.py` takes `logger` from `app.logger` and uses `logger.info`, and impact is asked about `logger` in `app/logger.py`
- **THEN** impact lists `a.py`
- **AND** the same holds for `logger/__init__.py`, which declares `logger` and is taken from `logger`

#### Scenario: A module stays the module when the package takes it by name and the module exports that name too
- **WHEN** `pkg/cli.py` declares `cli` and `load_dotenv`, `pkg/app.py` takes `cli` from `.` and uses `cli.load_dotenv`, and impact is asked about `load_dotenv` in `pkg/cli.py`
- **THEN** impact lists `pkg/app.py`

### Requirement: A star barrel yields to what it declares
A file that passes a name on by a star import and declares that name itself, or re-exports it by name from another file as `export { helper } from './b'` does, SHALL hide the name behind the star: asked about the name in the file behind, impact SHALL list no row for the barrel and SHALL not go on to the importers that take the name from it, because they take the barrel's own. Other names it passes on keep their row.

#### Scenario: A star barrel yields to a name it declares and exports itself
- **WHEN** `settings.py` has `from base import *` and the variable `DEBUG`, `app.py` imports `DEBUG` and `PORT` from `settings.py` and uses both in `run`, and impact is asked about `DEBUG` and about `PORT` in `base.py`
- **THEN** about `DEBUG` impact lists no file
- **AND** about `PORT` it lists `settings.py` with the reason `re-exports PORT from base.py` and `app.py` with the caller `run`

#### Scenario: A star barrel yields to a name another export of it takes by name
- **WHEN** `index.ts` has `export * from './a'` and `export { helper } from './b'`, `page.ts` imports `helper` and `other` from `index.ts` and uses both in `open`, `a.ts` and `b.ts` declare `helper`, `a.ts` declares `other`, and impact is asked about `helper` in `a.ts`, about `other` in `a.ts` and about `helper` in `b.ts`
- **THEN** about `helper` in `a.ts` impact lists no file
- **AND** about `other` it lists `index.ts` with the reason `re-exports other from a.ts` and `page.ts` with the caller `open`
- **AND** about `helper` in `b.ts` it lists `index.ts` with the reason `re-exports helper from b.ts` and `page.ts` with the caller `open`

### Requirement: A star barrel lists the declaration that reads the name it binds again
A file that passes a name on by a star import and binds that name again in a declaration that reads it, as `INSTALLED_APPS = INSTALLED_APPS + [..]` does after `from .base import *`, SHALL be listed with that declaration as the caller when impact is asked about the name in the file behind, though the name goes no further through the file. A declaration that binds the name without reading it SHALL not be listed.

#### Scenario: A star barrel lists the declaration that reads the name it binds again
- **WHEN** `settings.py` has `from base import *`, `DEBUG = False` and `INSTALLED_APPS = INSTALLED_APPS + [..]`, `app.py` imports both from `settings.py` and uses them in `run`, and impact is asked about `DEBUG` and about `INSTALLED_APPS` in `base.py`
- **THEN** about `DEBUG` impact lists no file
- **AND** about `INSTALLED_APPS` it lists `settings.py` with the reason `imports INSTALLED_APPS from base.py` and the caller `INSTALLED_APPS`, and not `app.py`
