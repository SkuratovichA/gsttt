---
name: gsttt
description: Use when a question is about how code is connected rather than what it says. What a change affects, what calls a function, class or method, which tests to run, what imports or depends on a file, how two files are linked, who serves or calls a route, GraphQL operation, gRPC method, tRPC procedure, table, queue or environment variable, and whether the index is complete and fresh enough to trust the answer.
---

# gsttt

gsttt keeps a graph of the files of a repository and of the contracts between them: imports, HTTP routes, GraphQL operations, gRPC methods, tRPC procedures, database tables, queues and environment variables. It reads the repository and nothing else, so the same tree always gives the same answer. Use it for questions about connections. Use grep and the editor for questions about what the text says.

## When to ask it

- **Before changing a file, a schema or a route**, ask what the change affects. The answer lists the affected files from the nearest to the farthest and puts the tests in a group of their own. Run those tests after the change.
- **Before changing one function, class, method or variable**, ask what calls it: ask impact about its name, in `names` beside the file that declares it in `paths`. The answer lists the direct callers, file by file, each with the declarations that refer to the name and the lines they start on, and the tests that call it. It is narrower than the answer about the whole file, which lists everything the file can reach. On the command line that is `gsttt impact path/to/file.ts --name getHash`, and a method is `Class.method`. To go further, ask again about a caller: its file in the paths and its name in the names, or give a larger depth.
- **To see who relies on a file**, ask for its dependents. **To see what a file relies on**, ask for its dependencies. Each entry carries the kind of its edge, and the dependencies also say which imports resolved to a package, to a builtin or to nothing, with the reason.
- **To see how two files are linked**, ask for the path between them. A step is an import, or a contract: a file that calls a route and the file that serves it.
- **To find who serves or calls a contract**, look up its key, such as `http GET /bookings/{}`, `graphql Query.bookings`, `table bookings` or `env API_URL`. Text that is not a key is searched in every key and lists the keys it finds.
- **To judge an answer**, ask what the index covers and how fresh it is.

## Reading an answer

- Every answer ends with two lines. The first says how much of the files involved the connectors read and what share of their imports resolved. The second says how fresh the index is.
- A last line that starts with `stale:` says how many files changed since the index was built. Refresh the index with `gsttt index` before you rely on the answer.
- A site marked as a possible miss is an import or a call that gsttt could not follow, such as an import of a file that does not exist or a request to a computed URL. Open it yourself before you decide that two files are unrelated.
- An answer about names lists direct callers and stops: a caller may keep its behaviour when what it calls changes, so what lies behind it is not claimed. A note says when the walk stopped with callers left to follow.
- An entry of such an answer reads `in run (line 3), stop (line 7)`: the declarations of that file that refer to the name, with the lines they start on. `at load` means that code which runs as the file loads refers to it, and a test file listed at load calls the name there.
- A note that a file declares no such name means a misspelt name or the wrong file, and nothing was reached from it. A note that a file is followed whole means its connector records no declarations, as Python's and Rust's do not yet, so its answer is the one about the whole file.
- A note that the callers of `Runner.start` are found through the declarations that refer to `Runner` means that a call through a value names no class. The list can hold a declaration that never calls the method, and miss one that gets an instance without naming the class. The walk does not follow a name reached through `globalThis`, `eval` or a computed key, or state of another module changed through an import.
- A path the answer calls `not in the index` was read by no connector. gsttt says nothing about it.
- An empty answer says what was searched. Nothing found means nothing found among those files, so check that the files you care about were among them.
- Long lists stop at 30 entries with a count of the rest. Ask for every entry when you need all of them.
- An answer stops at 25,000 characters. When it would be longer, its first line starts with `cut:`, its lists are shortened further, and the counts say what they leave out; ask a narrower question for the rest.
