---
name: gsttt
description: Use before you edit a file that other files import or call, and before you finish a change: lists what the edit affects and which tests to run. Also answers what calls a function, class or method, what imports a file, and who serves or calls a route, GraphQL operation, gRPC method, tRPC procedure, table, queue or environment variable.
---

# gsttt

gsttt keeps a graph of the files of a repository and of the contracts between them: imports, HTTP routes, GraphQL operations, gRPC methods, tRPC procedures, database tables, queues and environment variables. It reads the repository and nothing else, so the same tree always gives the same answer. Use it for questions about connections. Use grep and the editor for questions about what the text says.

## Before the first call

The gsttt tools may be deferred, listed by name only, and a deferred tool cannot be called until its schema is loaded. Load it first with ToolSearch and the query `select:mcp__plugin_gsttt_gsttt__impact`; add `,mcp__plugin_gsttt_gsttt__dependents` or another tool of gsttt to the same query when you need it.

## When to ask it

- **Before you edit a file, a schema or a route that other files import or call**, ask what the edit affects: `impact` with the file in `paths`. The answer lists the affected files from the nearest to the farthest and counts the tests. **Before you finish a change**, ask `impact` with `changed: true`: it lists the files your edits reach and the tests among them. Run those tests.
- **Before changing one function, class, method or variable**, ask what calls it: ask impact about its name, in `names` beside the file that declares it in `paths`. The answer lists the direct callers, file by file, each with the declarations that refer to the name and the lines they start on, and the tests among them. It is narrower than the answer about the whole file, which lists everything the file can reach. On the command line that is `gsttt impact path/to/file.ts --name getHash`, and a method is `Class.method`. To go further, ask again about a caller: its file in the paths and its name in the names, or give a larger depth. Across a contract that a caller serves the answer also lists the clients of that contract, which break without a type error, and that hop does not count toward the depth, which counts hops through code. The tests are listed with `changed: true` and counted otherwise, with a line such as `6 test files left out (ask with tests=true)`; pass `tests: true` to list them while you work out what to edit, or `tests: false` with `changed: true` to have them counted at the end. On the command line they are listed unless you pass `--no-tests`.
- **To see who relies on a file**, ask for its dependents. **To see what a file relies on**, ask for its dependencies. Each entry carries the kind of its edge, and the dependencies also say which imports resolved to a package, to a builtin or to nothing, with the reason.
- **To see how two files are linked**, ask for the path between them. A step is an import, or a contract: a file that calls a route and the file that serves it.
- **To find who serves or calls a contract**, look up its key, such as `http GET /bookings/{}`, `graphql Query.bookings`, `table bookings` or `env API_URL`. Text that is not a key is searched in every key and lists the keys it finds.
- **To judge an answer**, ask what the index covers and how fresh it is.

## Reading an answer

- Every answer ends with two lines. The first says how much of the files involved the connectors read and what share of their imports resolved. The second says how fresh the index is.
- A last line that starts with `stale:` says how many files changed since the index was built. Refresh the index with `gsttt index` before you rely on the answer.
- A site marked as a possible miss is an import or a call that gsttt could not follow, such as an import of a file that does not exist or a request to a computed URL. Open it yourself before you decide that two files are unrelated.
- An answer lists the direct callers, and the tests among them, and stops there for the rest, for a file as for a name, unless the request gives a depth: a caller may keep its behaviour when what it calls changes, so what lies behind it is not claimed. A note says when the walk stopped at the depth with callers left to follow. The first line says `within 1 hop through code`, and a hop across a contract is not counted in it. A test past the depth is counted, not listed, and so is every test of an answer that leaves them out: it says `N test files left out`. A large depth such as 4294967295 runs the walk to the end.
- An entry of such an answer reads `in run (line 3), stop (line 7)`: the declarations of that file that refer to the name, with the lines they start on. With a depth, a declaration that is farther from the name asked about than the file is listed says so, `in outer (line 5, distance 2)`. `at load` means that code which runs as the file loads refers to it, and a test file listed at load calls the name there; `at load (distance 2)` says it is farther away.
- A note that a file declares no such name means a misspelt name or the wrong file, and nothing was reached from it. A note that a file is followed whole means its connector recorded no declarations for it, as for a file read past syntax errors, so its answer is the one about the whole file.
- With `--changed`, a file the patch edits is walked from the declarations that changed, and the first line names them, `impact of formatUser in src/format.ts`. A file where no declaration changed is not traced, and the answer says so. A removed, renamed or moved declaration, a new file and a file that does not parse are walked whole.
- A note such as `3 files hold Runner and name no start; ask about Runner to list them` means that a call through a value names no class, so a file counts as a caller of `Runner.start` only when it reads `start` on some value or uses `Runner.start` itself; a file that holds `Runner` and never names `start` on any value is not listed, and the note counts it. When the answer holds several such members, one note names them all and says `the classes of` them. The list can hold a file that reads `start` on a value of another class, and miss one that calls the method through a name it builds at run time. The walk does not follow a name reached through `globalThis`, `eval` or a computed key, or state of another module changed through an import.
- A note such as `Aws.groups may run through GitProvider.groups in provider.ts, which 33 declarations override, so only the callers that name it are listed` means that a call on a value of unknown type does not tell one implementation of so many from another. Ask about `GitProvider.groups` to list those calls.
- A path the answer calls `not in the index` was read by no connector. gsttt says nothing about it.
- An empty answer says what was searched. Nothing found means nothing found among those files, so check that the files you care about were among them.
- Long lists stop at 30 entries with a count of the rest. Ask for every entry when you need all of them.
- An answer stops at 25,000 characters. When it would be longer, its first line starts with `cut:`, its lists are shortened further, and the counts say what they leave out; ask a narrower question for the rest.
