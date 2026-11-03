# Spec Delta

## ADDED Requirements

### Requirement: Members that hold a value of a class
A named field, or a method taking `self`, whose written type is a class SHALL be recorded as holding a value of that class under the name `Class.member`, looking inside `Option` and `Result`. `Self`, the class itself and a class of the standard library SHALL record nothing.

#### Scenario: A field of a class type holds a value of it
- **WHEN** a file has `pub struct Config { pub style: Style, pub maybe: Option<Style> }`
- **THEN** `Config.style` and `Config.maybe` are recorded as holding `Style`

#### Scenario: A method that returns a class holds a value of it
- **WHEN** a file has `impl Arg` with methods taking `&self` that return `&ValueRange`, `Option<ValueRange>` and `Result<ValueRange, Error>`
- **THEN** each method is recorded as holding `ValueRange`

#### Scenario: An associated function without self holds nothing
- **WHEN** a file has `impl Config { pub fn default_style() -> Style }`
- **THEN** nothing is recorded as held

#### Scenario: A member of its own class or of the standard library holds nothing
- **WHEN** a file has `impl Foo` with `fn with(self) -> Self`, `fn again(&self) -> Foo` and `fn len(&self) -> usize`, and a field `name: String`
- **THEN** nothing is recorded as held

### Requirement: Reads of one member on another
A member read right on a field or on what a method gives SHALL be recorded as the read `on.member`, seeing through parentheses, references, `?`, `.await` and calls that only hand on the value they hold.

#### Scenario: A member read right on another is read as a pair
- **WHEN** a file has `self.config.style.changes()`
- **THEN** the reads include `config.style` and `style.changes`

#### Scenario: A pair goes through a call that only hands on the value
- **WHEN** a file has `arg.range().expect("x").min()` and `arg.range()?.min()`
- **THEN** the reads include `range.min`

#### Scenario: A call that changes the value breaks the pair
- **WHEN** a file has `arg.range().map(f).min()`
- **THEN** the reads do not include `range.min`

#### Scenario: A chain through a typed field reaches the file that reads it
- **WHEN** `printer.rs` holds a `Config` with a field `style: Style` and calls `self.config.style.changes()` without naming `Style`, and `noise.rs` calls `changes` on a closure parameter without a written type
- **THEN** the impact of `Style.changes` lists `printer.rs` and not `noise.rs`
