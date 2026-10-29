---
name: gsttt
description: Use when a question is about how code is connected rather than what it says. What a change affects, which tests to run, what imports or depends on a file, how two files are linked, who serves or calls a route, GraphQL operation, gRPC method, tRPC procedure, table, queue or environment variable, and whether the index is complete and fresh enough to trust the answer.
---

# gsttt

gsttt keeps a graph of the files of a repository and of the contracts between them: imports, HTTP routes, GraphQL operations, gRPC methods, tRPC procedures, database tables, queues and environment variables. It reads the repository and nothing else, so the same tree always gives the same answer. Use it for questions about connections. Use grep and the editor for questions about what the text says.

## When to ask it

- **Before changing a file, a schema or a route**, ask what the change affects. The answer lists the affected files from the nearest to the farthest and puts the tests in a group of their own. Run those tests after the change.
- **To see who relies on a file**, ask for its dependents. **To see what a file relies on**, ask for its dependencies. Each entry carries the kind of its edge, and the dependencies also say which imports resolved to a package, to a builtin or to nothing, with the reason.
- **To see how two files are linked**, ask for the path between them. A step is an import, or a contract: a file that calls a route and the file that serves it.
- **To find who serves or calls a contract**, look up its key, such as `http GET /bookings/{}`, `graphql Query.bookings`, `table bookings` or `env API_URL`. Text that is not a key is searched in every key and lists the keys it finds.
- **To judge an answer**, ask what the index covers and how fresh it is.

## Reading an answer

- Every answer ends with two lines. The first says how much of the files involved the connectors read and what share of their imports resolved. The second says how fresh the index is.
- A last line that starts with `stale:` says how many files changed since the index was built. Refresh the index with `gsttt index` before you rely on the answer.
- A site marked as a possible miss is an import or a call that gsttt could not follow, such as an import of a file that does not exist or a request to a computed URL. Open it yourself before you decide that two files are unrelated.
- A path the answer calls `not in the index` was read by no connector. gsttt says nothing about it.
- An empty answer says what was searched. Nothing found means nothing found among those files, so check that the files you care about were among them.
- Long lists stop at 30 entries with a count of the rest. Ask for every entry when you need all of them.
- An answer stops at 25,000 characters. When it would be longer, its first line starts with `cut:`, its lists are shortened further, and the counts say what they leave out; ask a narrower question for the rest.
