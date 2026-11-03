# Proposal: A read on a value of a type outside the module is no member of a type here

## Why

Asked about a method, gsttt lists every Go file that reads a member of that name on a value whose type it cannot tell. A chain that stands on a value of the standard library, `r.Header.Get("x")` with `r *http.Request`, `w.Header().Get("x")`, `req.Header.Set(...)` after `req, _ := http.NewRequest(...)` or `file.Name()` after `os.CreateTemp`, records a read of `Get`, `Set` or `Name`, and so lists the files that read those names for a method of a type of the module. A read that stands on a value of a type outside the module can never be a call of a method of the module, and a list of hundreds of files for one function is no better than a text search.

## What Changes

- Go: a member read on a chain of fields, calls, indexes and parentheses that stands on a value of a type outside the module records no read. The chain stands outside when its head is a name that holds a type of an import that is not of the repository, a variable of such a package (`os.Stdout`, `http.DefaultClient`), a field a struct of the file writes with such a type, a call of a function of the standard library, or a member read or called on a value that stands outside. Its operands and arguments are still walked.
- Go: a name bound to a call of a function of the standard library holds a value outside the module. Standard library means an import that is not of the repository and whose path has no dot in its first segment, bar `slices`, `maps`, `cmp`, `iter`, `sync`, `sync/atomic`, `unique` and `weak`, whose functions hand back the type of the caller. A package outside the standard library gives nothing, because the name of a function does not tell what it returns. A name assigned after it is declared is not one.
- A type written with type arguments, `atomic.Pointer[T]`, is not outside, because a type parameter may stand behind it. A type-asserted value stands on the asserted type.
- Limits that stay: a method of a type of the module that runs through an interface of the standard library, such as `w.Write` on a wrapper reached as `http.ResponseWriter`, which the references of a compiler do not count either; the return type of a function of the module, which a reader of one file does not see, so `flags := cmd.NonInheritedFlags(); flags.SetOutput(buf)` stays a read; and `c, _ := CreateTestContext(w); c.Request.Header.Set(...)`, whose root is unknown.
- Versions: `READER_VERSION` of the Go connector goes from 5 to 6, because the facts of the same text change.

## Impact

- Capability `go`.
- `Walk::selector` and the new `stands_outside` in `crates/lang-go/src/walk.rs`, `Head::generic` and `Binding::outside` in `crates/lang-go/src/scope.rs`, `FileCx::is_outside` and `ImportInfo::gives_outside_values` in `crates/lang-go/src/file.rs`.
- A file that reads members of types of the module as before gives the same answers; fewer files list a method whose name the standard library shares.
