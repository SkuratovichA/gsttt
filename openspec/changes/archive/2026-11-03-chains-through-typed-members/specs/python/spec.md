## ADDED Requirements

### Requirement: Members that hold a value of a class
A property, an annotated or assigned field and a method with a return annotation that name a class SHALL be recorded as holding a value of that class, and a field assigned from such a method holds what the method returns. A member of a builtin type SHALL hold nothing.

#### Scenario: A property that names a class holds a value of it
- **WHEN** a property is annotated to return a class
- **THEN** the file records the member as holding a value of the class

#### Scenario: A field annotated or assigned a class holds a value of it
- **WHEN** a field is annotated with a class or assigned a call of it
- **THEN** the file records the field as holding a value of the class

#### Scenario: A method that names a class as its return holds a value of it
- **WHEN** a method is annotated to return a class
- **THEN** the file records the method as holding a value of the class

#### Scenario: A field assigned from a method of the class holds what the method returns
- **WHEN** a field is assigned the call of a method of its class that returns a class
- **THEN** the file records the field as holding a value of that class

#### Scenario: A member of a builtin type holds nothing
- **WHEN** a property, a field or a method names only builtin types
- **THEN** the file records no holder for it

#### Scenario: A property that names a builtin ends the chain
- **WHEN** a method reads a member on a property that names a builtin type
- **THEN** the file records no read of the member

#### Scenario: A chain through typed properties reaches the file that reads it
- **WHEN** a file reads `self.poetry.file.read()` through two properties that name their class
- **THEN** impact of the method it reaches lists that file, and not one that reads the names apart

### Requirement: A nested class is no holder
A class nested in a class SHALL stay an heir of its bases under its dotted name, and SHALL NOT be recorded as a member that holds a value.

#### Scenario: A nested class stays an heir of its base and holds nothing
- **WHEN** a class holds a nested class that extends a class the file imports
- **THEN** the file records the nested class as an heir of its base and records no holder

### Requirement: Reads of one member on another
An attribute read right on another SHALL be recorded as a read of both and of the pair `on.member`, where the pair is recorded inside the walk of a value that is not a builtin. A chain on a value the file shows is a builtin SHALL record no pair.

#### Scenario: An attribute read right on another is a read of both
- **WHEN** a file reads `self.poetry.file.read()`
- **THEN** it records the reads `poetry`, `file`, `read` and the pairs `poetry.file` and `file.read`

#### Scenario: An attribute read on what a method gives is read on the method
- **WHEN** a file reads `x.get().name`
- **THEN** it records the read `get.name` beside `get` and `name`

#### Scenario: A chain on a name the file binds is read one member on another
- **WHEN** a file reads `app.config.from_mapping(x)` with `app` imported from another file
- **THEN** it records the read `config.from_mapping` and neither `config` nor `from_mapping`

#### Scenario: A chain on a builtin value records no read of one on another
- **WHEN** a file reads a member of a member on a value it shows is a builtin
- **THEN** it records no pair
