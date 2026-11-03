# Spec Delta

## REMOVED Requirements

### Requirement: Names of an import
**Reason**: Listing a name once for a statement that imports it twice hides one of the two names it binds, and the walk needs both to find what uses either; its one scenario names the dedupe that stops.
**Migration**: "Names an import takes and binds" lists a name for each name an import binds; the scenario "A name imported twice is listed for each name it binds" replaces "A name imported twice is listed once".

## ADDED Requirements

### Requirement: Names an import takes and binds
An import SHALL list the names it takes and the names it binds, in the order written and of one length: `from m import a as b, c` takes `a` and `c` and binds `b` and `c`, `import a.b` takes everything and binds `a.b`, `import a.b as c` binds `c`, and a star import binds none. A name imported twice SHALL be listed for each name it binds. A `from` import SHALL be flagged as one whose names may be modules, because `from pkg import sub` binds `pkg/sub.py` when the package has it.

#### Scenario: A name imported twice is listed for each name it binds
- **WHEN** a file has `from bookings import views as first, views as second`
- **THEN** its one import of `bookings` takes the name `views` twice and binds `first` and `second`
- **AND** the import resolves to `bookings/views.py`

#### Scenario: A plain import lists the module it binds
- **WHEN** a file has `import os`, `import a.b`, `import a.b as c`, `from m import x as y, z` and `from pkg import *`
- **THEN** the imports list the names they take and bind as `*` and `os`, `*` and `a.b`, `*` and `c`, `x, z` and `y, z`, and `*` and nothing

#### Scenario: A from import may name a module
- **WHEN** a file has `from pkg import sub`, `import pkg.sub`, `import os` and `from pkg import *`
- **THEN** only the import of `sub` is flagged as one whose names may be modules

### Requirement: Declarations
The connector SHALL declare every function, class, method, class variable, assignment and `type` alias with its lines, a method and a class variable under the name of their class as `Class.name`, a decorator being part of the function it decorates. An attribute assigned on `self` SHALL be declared under its class, and `if __name__ == "__main__":` SHALL be declared as `__main__`.

#### Scenario: A function, a class and a variable are declared with their lines
- **WHEN** a file has the variables `X` and `Y: int`, the function `f` that returns `X`, and the class `K`
- **THEN** it declares `X`, `Y`, `f` and `K` with their line ranges, and `f` uses `X`
- **AND** its facts match the reviewed snapshot

#### Scenario: A method and a class variable are declared under their class
- **WHEN** a class `K` has the variables `attr` and `other: int`, the method `m` that returns `self.other`, and a static method `s` with a decorator
- **THEN** it declares `K`, `K.attr`, `K.other`, `K.m` and `K.s`, and `K.s` starts on the line of its decorator
- **AND** `K.m` uses `K.other`

#### Scenario: An attribute assigned on self is declared under its class
- **WHEN** a class has an `__init__` that assigns `self.v` and a method `m` that assigns it again
- **THEN** it declares `K.v`, and a use of `K.v` is a use of `K.__init__` and `K.m`

#### Scenario: A type alias is declared
- **WHEN** a file has `type Alias = list[int]`
- **THEN** it declares `Alias`

#### Scenario: A main guard is declared as __main__
- **WHEN** a file has `def f()` and an `if __name__ == "__main__":` block that calls `f()`
- **THEN** it declares `f` and `__main__` with the lines of the block, and `__main__` uses `f`
- **AND** nothing runs on load

### Requirement: What a declaration uses
A declaration SHALL use the names it reads as the file writes them, so `y` for `from m import x as y` and `a.b.w` whole. It SHALL use the initialiser of a class it calls when the class defines one or is imported, and the method it calls on `self`, through `super()`, on a value made from a class or on a parameter annotated with a class. A name that a local hides, as a parameter or an assignment does, SHALL not be a use of the name of the file.

#### Scenario: A declaration uses the names it reads
- **WHEN** a function reads `y(p)`, `c.q` and `a.b.w` after `from m import x as y`, `import a.b` and `import a.b as c`
- **THEN** it uses `a.b.w`, `c.q` and `y`

#### Scenario: A local name hides the name of the file
- **WHEN** a file has the variable `X` and three functions, one with a parameter `X`, one that assigns `X`, and one that reads `X`
- **THEN** the first two use nothing and the third uses `X`

#### Scenario: Calling a class uses its initialiser
- **WHEN** a function calls a class of the file that defines `__init__`
- **THEN** it uses the class and `K.__init__`

#### Scenario: A call on self uses the method of its class
- **WHEN** a method calls `self.go()` and its class defines `go`
- **THEN** it uses `K.go`

#### Scenario: A call on a value made from a class uses the method of the class
- **WHEN** a function has `k = K()` and calls `k.m()`, and `K` defines `m` and no `__init__`
- **THEN** it uses `K` and `K.m`

#### Scenario: A call through super uses the method of the base
- **WHEN** `B(A)` defines `go`, which calls `super().go()`, and `A` defines `go`
- **THEN** `B.go` uses `A.go`

#### Scenario: An annotated parameter gives the type of the value
- **WHEN** a function has the parameters `a: A` and `b: "A"` and calls `a.go()` and `b.go()`
- **THEN** it uses `A` and `A.go`

### Requirement: What gives a value its class
A value SHALL have a class only when everything that gives it one names the same class, a class of the file or one an import brings in, alone or with `None`. A name bound by an assignment, as a `TypeVar` or an alias of a union, a name of `typing`, `typing_extensions`, `collections.abc` or `types`, and several different classes in a union, an `or`, a conditional expression or the assignments to a field SHALL give none, and a member read on the value SHALL be read by name.

#### Scenario: A loose annotation is no class
- **WHEN** a function takes a parameter annotated `Any`, one annotated with a `TypeVar` bound to a class, and one annotated with an alias of `Union[A, B]`, and calls `run` on it
- **THEN** the file records the read `run` and the function uses no member of a class
- **AND** a parameter annotated with a class `A` of an import uses `A.run`

#### Scenario: A value of several classes is no class
- **WHEN** a function calls `run` on a parameter annotated `Dog | Cat`, `Union[Dog, Cat]` or `Optional[Dog | Cat]`, on `x or Dog()`, on a conditional expression of `Dog()` and `Cat()`, on a field assigned `Dog()` in one method and `Cat()` in another, on a field assigned `repo or Dog()`, and on a field annotated `Dog | Cat`
- **THEN** the file records the read `run`, and no declaration uses `Dog.run` or `Cat.run` for it
- **AND** a parameter annotated `Optional[Dog]` or `Dog | None`, and a conditional expression of `Dog()` twice, use `Dog.run`

### Requirement: Reads of members
A member read on a value whose type the file does not show SHALL be recorded as a read of the member's name, and the declaration SHALL use no member of any class for it. A member read on a builtin value SHALL be no read: a local assigned a literal, a comprehension or a call of a builtin type, and a local, parameter or field annotated with one, or `Optional` of one. A member read on `self` that a base the file does not declare may hold SHALL stay a read.

#### Scenario: A member read on a value of unknown type is read by name
- **WHEN** a function takes the parameter `d` with no annotation and calls `d.unknown()`
- **THEN** the file records the read `unknown`, and the function uses nothing

#### Scenario: A list is no receiver of a member
- **WHEN** a function assigns a list, a call of `list`, a comprehension and an f-string to locals, takes parameters annotated `list[int]`, `Optional[dict]` and `int | None`, and calls `append`, `sort`, `pop`, `strip`, `get` and `bit_length` on them
- **THEN** the file records no read
- **AND** a parameter with no annotation and a local assigned `make()` on which `append` and `sort` are called give the reads `append` and `sort`

#### Scenario: A field annotated with a builtin is no receiver of a member
- **WHEN** a class annotates `items: list[str]` and `label: Optional[str]`, its `__init__` assigns `self.names` from a parameter annotated `dict`, `self.count` from one annotated `int | None`, `self.log` from `[]` and `self.other` from a parameter with no annotation, and a method calls a member on each
- **THEN** the file records only the read of the member called on `self.other`
- **AND** the method uses the five fields

#### Scenario: A builtin local that is bound again is read by name
- **WHEN** a function assigns a list to five locals, and binds one again by a loop, one by an assignment of `make()`, one by `if` and `else` with `make()` in one branch, and one by `+=`, and calls a member on each, and calls one on the fifth that nothing binds again
- **THEN** the file records the reads of the first four members and not that of the fifth

#### Scenario: A member read beside a base the file does not declare is kept
- **WHEN** `Base(ABC)`, with `ABC` imported, has a method `go` that calls `self.step()` and defines no `step`, and `Local` defines `step` and a method `go` that calls `self.step()`
- **THEN** the file records the read `step`, and `Local.go` uses `Local.step`

### Requirement: Where a local stops holding its type
A local that holds an instance of a class SHALL stop holding it where the name is bound again, by a loop, `with … as`, `except … as`, a `match` capture, a walrus, `del`, `+=` or another assignment, and a member read after it SHALL be a read by name. A walrus in a comprehension SHALL bind its name in the function around it. An assignment of a name that an import binds, as `X = wrap(X)`, SHALL use the import.

#### Scenario: A loop, a with, an except, a walrus and a del end the type of a local
- **WHEN** parameters annotated `A` are bound again by a loop, a `with`, an `except`, a walrus, a `del` followed by an assignment, and a `match` capture, and `run` is called on each, and `run` is called on one that is not bound again
- **THEN** only the one that is not bound again uses `A.run`, and `run` is a read for the others

#### Scenario: A walrus in a comprehension binds a name of the function
- **WHEN** a function with a parameter `p: A` binds `p` by a walrus in a comprehension and calls `p.run()`, and another function binds `X` by a walrus in a comprehension and returns it, while the file has the variable `X`
- **THEN** `run` is a read, and the second function uses nothing

#### Scenario: An assignment of a name an import binds uses the import
- **WHEN** a file has `from m import X, wrap` and `X = wrap(X)`, and the variable `Y` and `Y = wrap(Y)`
- **THEN** `X` uses `X` and `wrap`, and `Y` uses `wrap` only

### Requirement: A local given values of different types
A local bound to values of different types anywhere in its function SHALL hold no class at any read in it, whether the bindings are the two branches of an `if`, a parameter bound again in a branch, or a binding after the read in a loop that the next pass reaches; a member read on it SHALL be read by name. A local bound each time to the same type, or only once, SHALL keep it.

#### Scenario: A local given values of different types is unknown wherever it is read
- **WHEN** a function binds a local to `A()` in one branch and `B()` in the other, another rebinds a parameter annotated `A` to `B()` in a branch, and a third reads a local in a loop before binding it to `B()` after binding it to `A()`, and each calls `run` on the local
- **THEN** the file records the read `run`, and no function uses `A.run` or `B.run`
- **AND** a local bound to `A()` twice, a parameter annotated `A` bound to `A()` in a branch, and a local bound once, use `A.run`

### Requirement: Where a comprehension and a class look
The first iterable of a comprehension SHALL be read where the comprehension stands, so that in a class body it reads a variable of the body, and the other iterables SHALL not. A class local to a function SHALL be an heir of nothing and override nothing, because it is no declaration of the file.

#### Scenario: A comprehension in a class body reads a variable of the body for its first iterable
- **WHEN** a class has `xs = [1]`, `ys = [x for x in xs]` and `zs = [(x, y) for x in [1] for y in xs]`
- **THEN** `K.ys` uses `K.xs` and `K.zs` uses nothing

#### Scenario: A class local to a function is an heir of nothing
- **WHEN** a file has `class Top(Base)` and a function that defines `Top(Base)` and `Local(Base)` in its body
- **THEN** the file records the one heir and the one override `Top` of `Base`

### Requirement: Strings in annotations
A string in an annotation SHALL be read as a type, except in a `Literal` and in the metadata that follows the type of an `Annotated`, which hold values.

#### Scenario: A string of a Literal and the metadata of an Annotated are no names
- **WHEN** a file has the class `Order` and four functions with a parameter annotated `Literal["Order"]`, `Annotated[int, "Order"]`, `Annotated["Order", "meta"]` and `"Order"`
- **THEN** the first two use only `Literal` and `Annotated`, and the other two use `Order`

### Requirement: Code that runs when a file loads
A call at the top level of a file, in a decorator or in the body of a class SHALL be recorded as run on load, and a call in the body of a function SHALL not.

#### Scenario: A top-level call runs when the file loads
- **WHEN** a file has `from m import run` and `run()`
- **THEN** `run` is among what runs on load

#### Scenario: A decorator and a class body run when the file loads and a function body does not
- **WHEN** a file imports `run`, `stop` and `setup`, decorates a function with `@setup.reg`, calls `stop()` in its body, and a class calls `run()` in its body and `stop()` in a method
- **THEN** `run` and `setup.reg` run on load and `stop` does not

### Requirement: Exports of a module
A module SHALL export every name it binds at its top level, its imports among them, because `from m import x` takes an `x` that `m` imports as it takes one that `m` defines, and the strings its `__all__` is given by an assignment, `+=`, `extend` or `append`, in a block too. The names a star import brings in SHALL not be listed.

#### Scenario: A module exports every name it binds and those of __all__
- **WHEN** a module has `from .a import A`, `from .b import b as bb`, `import os.path`, `from d import e`, an import and an assignment of `fast` in a `try`, `__all__ = ["A", "zz"]`, the variable `X`, and a function `f` that imports `local`
- **THEN** it exports `A`, `X`, `__all__`, `bb`, `e`, `f`, `fast`, `os` and `zz`, and not `local`

#### Scenario: __all__ is read in every literal form
- **WHEN** a module gives `__all__` strings by `=` with a list, a tuple, a set and a sum, by an annotated assignment, by `+=`, by `extend` and by `append`, in a block too
- **THEN** it exports each of the strings

### Requirement: What takes the members of a class
A class SHALL be an heir of each base it lists that the file binds, and override the members of that base. A name bound to a call of a class, as `app = FastAPI()`, SHALL be an heir of that class and override nothing.

#### Scenario: A class base gives an heir and an override
- **WHEN** a file has the class `A`, and `class B(A, Mixin)` where nothing binds `Mixin`
- **THEN** `B` is an heir of `A` and overrides the members of `A`, and nothing is recorded for `Mixin`

#### Scenario: A value made from a class is an heir and overrides nothing
- **WHEN** a file has `from fa import FastAPI` and `app = FastAPI()`
- **THEN** `app` is an heir of `FastAPI` and overrides nothing

### Requirement: Files with a star import or a syntax error
A file with a syntax error from which the parser recovered SHALL record no declarations and no exports and keep the imports it read. A file with a star import SHALL record its declarations and its exports, unless it reads a name that nothing in it binds and that is no builtin, which the star may have brought in, and then it SHALL record neither. Such a file is followed whole.

#### Scenario: A star barrel records the names it declares
- **WHEN** a file has `from pkg import *`, the variable `X` and the function `f` that returns `X`
- **THEN** it declares `X` and `f` and exports them, and its one import takes `*` and binds nothing

#### Scenario: A file reading an unbound name next to a star import is followed whole
- **WHEN** a file has `from pkg import *` and a function that returns `X + Y`, which nothing in the file binds
- **THEN** the file records no declarations and no exports, and keeps its import

#### Scenario: A syntax error leaves the declarations unrecorded
- **WHEN** a file has `def f(:` and then `from a import b`
- **THEN** the file records no declarations and no exports, and its import of `b` from `a` is kept

### Requirement: A name a star import may bring in
In a file with a star import, the first binding of a top-level name SHALL use that name where it reads it, as `X = X + [1]` after `from .base import *` does, because the star may have brought the name in. A binding that reads nothing of the name SHALL use nothing, as `X = False` does, a later binding of the name SHALL read the name of the file, and a file with no star import SHALL use nothing for it.

#### Scenario: The first binding of a name in a file with a star import uses the name the star may bring in
- **WHEN** a file has `from .base import *`, `INSTALLED_APPS = INSTALLED_APPS + ["x"]`, `INSTALLED_APPS = INSTALLED_APPS + ["y"]`, `DEBUG = False` and `PORT = 80 + 1`, and another file has `X = X + 1` and no star import
- **THEN** the first `INSTALLED_APPS` uses `INSTALLED_APPS`, `DEBUG` and `PORT` use nothing, and `X` uses nothing

### Requirement: Blind spots of a name
The provides of every Python project SHALL name, as blind spots of the uses of a name, the pytest fixtures that a test reaches by a parameter name, the names that Django reads from a string, and the functions replaced at run time.

#### Scenario: The provides name the uses of a name that no import links
- **WHEN** the coverage of a Python project with no pack is read
- **THEN** its provides name pytest fixtures, names that Django reads from a string, and monkeypatching among the blind spots

### Requirement: The code of a Python file
The code of a file, which `impact --changed` compares between two versions, SHALL be its text with the comments and the imports blanked, each line keeping its place. The indentation and the whitespace inside a string SHALL stay part of it, so that moving a statement into a block or changing a string is a change. A file with a syntax error, and a file that is no Python source, SHALL have no code.

#### Scenario: A comment or an import does not change the code
- **WHEN** a file changes `import os` to `import sys`, a comment to another comment, and drops a comment at the tail of a line
- **THEN** its code is the same, without the space around the lines

#### Scenario: Indentation is part of the code
- **WHEN** a statement moves out of an `if` block by a dedent
- **THEN** the code differs, and has the same number of lines

#### Scenario: Whitespace in a string is part of the code
- **WHEN** a file changes `"a  b"` to `"a b"`, or has a triple-quoted string with indented, empty and tabbed lines
- **THEN** the code differs, and a line break inside the string stays a line of the code

#### Scenario: A syntax error gives no code
- **WHEN** a file has `def f(:` and `setup.cfg` is asked for its code
- **THEN** neither has a code
