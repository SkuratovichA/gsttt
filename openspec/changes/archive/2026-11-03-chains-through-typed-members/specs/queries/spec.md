## ADDED Requirements

### Requirement: Reads through a member that holds a value
A name asked about SHALL reach a file that reads a chain of up to four names, each right on the previous, when the chain goes through members that hold a value of a class and the file imports a class of the chain. A file that reads the names apart SHALL NOT be a caller. The reads SHALL be those of the file as a whole, which do not say which value a read is on.

#### Scenario: A file that reads the chain through a member that holds a value is a caller
- **WHEN** a member holds a value of the class asked about and a file that imports a class of the chain reads that member and then the name asked about right on it
- **THEN** the file is a caller of the name

#### Scenario: A file that reads the names apart is no caller through the member
- **WHEN** a file reads the member and reads the name asked about, but never the one right on the other
- **THEN** the file is no caller through the member

#### Scenario: A file that imports no class of the chain is no caller
- **WHEN** a file reads the whole chain but imports no class of it
- **THEN** the file is no caller

#### Scenario: A chain goes through two members
- **WHEN** one member holds a value of a class whose member holds a value of the class asked about, and a file reads all three names one on the next
- **THEN** the file is a caller

#### Scenario: A chain goes through an heir of the class that holds the member
- **WHEN** a subclass of the class that holds the member is the class a file imports
- **THEN** the file is a caller through the member

#### Scenario: A chain goes through an heir of the class asked about
- **WHEN** a member holds a value of a subclass of the class asked about
- **THEN** a file that reads the chain is a caller

#### Scenario: A chain stops at four names
- **WHEN** a chain needs five names to reach the name asked about
- **THEN** the file that reads the five is no caller, and one that reads four is

#### Scenario: Members that hold each other end the walk
- **WHEN** two members hold a value of the class of the other
- **THEN** the walk ends and lists the files it found

#### Scenario: A crowded head starts no chain
- **WHEN** a head name is the member of too many classes
- **THEN** no chain starts at it
