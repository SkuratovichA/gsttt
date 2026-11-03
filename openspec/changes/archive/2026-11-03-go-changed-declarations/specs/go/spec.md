# Spec Delta

## ADDED Requirements

### Requirement: The code of a Go file
The code of a file, which `impact --changed` compares between two versions, SHALL be its text with the comments and import declarations blanked and each run of spaces and tabs inside a line squeezed to one space. A string, a raw string, a rune, a `//go:` or `// +build` line and the comments above `import "C"` SHALL stay. A file with a syntax error and `go.mod` SHALL have none. A shift of `iota` and a function moved in its file are not told apart from a change of the file.

#### Scenario: A comment or an import does not change the code
- **WHEN** a file changes `import "os"` to `import "fmt"`, a comment to another comment, and drops a comment at the tail of a line
- **THEN** its code is the same, without the space around the lines

#### Scenario: A realigned block does not change the code
- **WHEN** gofmt realigns `a    int` of a struct, `"a":   1` of a map literal and `A   = 1` of a constant group
- **THEN** the code is the same, and a field that is added still differs

#### Scenario: Whitespace in a string is part of the code
- **WHEN** a file changes `"a  b"` to `"a b"`, `' '` to a rune holding a tab, or has a raw string with indented, empty and tabbed lines
- **THEN** the code differs, and a line break inside the raw string stays a line of the code

#### Scenario: Tokens that a space keeps apart stay apart
- **WHEN** a file changes `a &^ b` to `a & ^b`, or `a <- b` to `a < -b`
- **THEN** the code differs

#### Scenario: A directive is part of the code
- **WHEN** a file changes `//go:build linux` to `//go:build darwin`, `//go:embed a.txt` to `//go:embed b.txt`, or `// +build linux` to another
- **THEN** the code differs

#### Scenario: The preamble of cgo is part of the code
- **WHEN** the comment above `import "C"` changes from `#include <a.h>` to `#include <b.h>`
- **THEN** the code differs

#### Scenario: A file with a syntax error has no code
- **WHEN** a file has `func f( {` and `go.mod` is asked for its code
- **THEN** neither has a code
