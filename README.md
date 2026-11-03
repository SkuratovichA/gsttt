<p align="center">
  <img src="assets/gsttt.svg" width="128" height="128" alt="An 8-bit ghost, the gsttt logo">
</p>

# gsttt

It answers one question without guessing: what does this change affect?

[Website](https://gsttt.arstoien.com) · [Releases](https://github.com/SkuratovichA/gsttt/releases) · [Claude Code plugin](plugin/README.md)

gsttt builds a graph of the files in a repository and of the contracts between them: imports, HTTP routes and the calls that reach them, GraphQL, gRPC and tRPC operations, database tables and environment variables. It follows them across packages, languages and services. No model runs anywhere in it, so the same tree always gives the same answer, and every answer says how much of the repository it covers and how fresh the index is.

<p align="center">
  <img src="assets/impact.gif" alt="gsttt indexes a small repository and answers what a change to a FastAPI route in Python affects: the app that includes the router, the TypeScript that fetches the route, the page and the entry point that import that TypeScript, and the test of the route">
</p>

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/SkuratovichA/gsttt/main/install.sh | sh
```

The installer checks the SHA-256 of what it downloads and puts `gsttt` in `~/.local/bin`, or in `GSTTT_INSTALL_DIR` when that is set. Linux and macOS, on x86-64 and arm64.

To update, run `gsttt update`. It installs the newest release over the `gsttt` that runs and updates the [Claude Code plugin](plugin/README.md) to match. It says what it changed, such as `Updated the plugin in ~/.claude (user): 9b6517d98ed1 → 4c1f2aa3b5c6`, and asks you to restart the Claude Code sessions that are running only when something changed. All of that goes to stderr, and stdout stays empty. `gsttt update 0.1.42` installs that version instead and leaves the plugin as it is, because the plugin follows the newest release.

```sh
gsttt update
```

Without the installer, each release is also a container image on ghcr.io, for CI pipelines and for MCP clients that start their servers through `docker run`:

```sh
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/repo" ghcr.io/skuratovicha/gsttt impact --changed
docker run --rm -i --user "$(id -u):$(id -g)" -v "$PWD:/repo" ghcr.io/skuratovicha/gsttt mcp   # the MCP server, over standard input and output
```

`--user` is there because the index is written to `.git/gsttt` of the repository, which would otherwise belong to root.

Then, in a repository:

```sh
gsttt impact --changed    # what the changes since HEAD affect
gsttt impact src/api.ts   # what a change to one file would affect
gsttt coverage            # what gsttt read, and what it cannot see in this repository
```

The first answer builds the index, and every later one rereads only the files that changed. `gsttt --help` lists the other questions: `dependents`, `dependencies`, `path` and `contract`.

To give Claude Code the same answers, add the plugin from this repository:

```text
/plugin marketplace add SkuratovichA/gsttt
/plugin install gsttt@gsttt
```

It brings the gsttt MCP server and a skill that says when to ask it; [plugin/README.md](plugin/README.md) has the details.

## Speed

gsttt builds the import graph of a whole repository from scratch 10 to 60 times faster than dependency-cruiser, madge and codebase-memory-mcp: 92 ms for vite, where they take one to two seconds.

<p align="center">
  <img src="assets/vite.gif" alt="gsttt indexes vite, 2,878 files, from scratch; after a one-line edit it rereads only that file, and then answers what the edit affects: 9 files and 2 tests">
</p>

Every number, with the command that measured it, is in [`bench/results/benchmarks.json`](bench/results/benchmarks.json), and `cargo xtask bench --gsttt <path to gsttt>` measures them again.

## What it reads

- Languages: TypeScript and JavaScript, Python, Rust, Go.
- Contracts: OpenAPI, GraphQL, protobuf, Prisma, SQL, `.env` files.
- Framework knowledge lives in TOML rule packs, and any other language can join as an external connector that speaks JSON lines.
- Configuration is TOML, in `.gsttt/config.toml`, with a JSON Schema that checks it in the editor; `gsttt init` writes both.

## Known gaps

`gsttt coverage` prints every connector gap below that applies to your repository, so an answer never hides one. The ones marked *not reported* are those it cannot print.

- **Linux and macOS only.** Windows is neither built nor tested.
- **A framework is read only where a manifest declares it.** A project's routes, clients and tables are read by the rules of the frameworks its `package.json`, `pyproject.toml`, `setup.py`, `setup.cfg`, requirements files or `Cargo.toml` list as dependencies; a framework that is installed some other way, such as by a Dockerfile, is not read, and its imports show up in `gsttt coverage` as a package that is in no manifest.
- **Nothing computed at run time is followed:** URLs, route prefixes, table names, SQL and environment variable names held in variables or built from strings, and dependency-injection containers. Code, routes and schemas generated at build time are not read either.

<details>
<summary>TypeScript and JavaScript</summary>

- Routes defined by the file system (Next.js, Remix, SvelteKit).
- A NestJS global prefix is applied only when it is a literal set on an app that `NestFactory.create(<name>)` makes in the same file, and the `exclude` option is not read, so excluded routes get the prefix too; `RouterModule` paths are not read. Any other `setGlobalPrefix` call, and one that passes options, is reported as a site gsttt could not read.
- NestJS modules are followed through `@Module` `imports` and `controllers` written as names or `forwardRef(() => Name)`; modules made by a call (`forRoot`, `register`), names read from a namespace (`ns.Module`) and anonymous default exports are not.
- A NestJS `@Controller({ path: [...] })` serves its routes under the first path of the array that is text only, where `@Controller([...])` serves them under every path.
- A NestJS controller serves the routes of the class it extends only when that class is named directly (not `ns.Base`), carries no `@Controller` of its own, and is its direct base.
- Routers mounted from another package, and routes on an app or router received as a function parameter: the app or router must be made in the file that declares the routes, or in a file of the same project that it is imported from. A name is looked up in the variables the file makes with no scope resolution, so a parameter or a variable inside a function that is named like a router or app the file makes is read as that one.
- A mount on a member of an imported name (`server.app.use('/x', router)`) or on a name from a package: an app or router takes a mount only when the file makes it or imports it by its own name, and the routes of a router that no read mount reaches are served at the paths they are written with.
- A mount path array is read for its literal elements only, and routers that mount each other in a cycle are followed until the cycle closes.
- An `axios.create` client that is received as a parameter or injected, or that a file of another package makes: the client must be made in the file that calls it, or in a file of the same project that it is imported from. A name is looked up in the variables the file makes with no scope resolution, so a parameter or a variable inside a function that is named like a client the file makes is read as that one.
- A test stand-in for a service (`test_double` of a rule, `ConfigService` for the `nestjs-config` pack) is seen only in the file that reads it, and only when it is provided by an object with `provide` and a `use*` property or by `overrideProvider`, so a `const configService = { get: jest.fn() }`, a provider in a shared helper and a renamed import keep the read.
- A Kafka producer or consumer (`kafka` pack) is read only when its receiver is declared with a type the pack lists (`ClientKafka`, `ClientProxy`, `KafkaProducerService`, `Producer`, `Consumer`) as a parameter, a class property, a constructor parameter property or a variable made with `new`, so `const producer = kafka.producer()` is not read, and neither are topics given as an array to `subscribe` (`{ topics: [...] }`).
- GraphQL resolvers built at run time, and schema-first resolvers outside a resolvers map.
- tRPC routers merged or built at run time, router entries that are spread, read from a member (`other.router`) or built by another call, and clients named anything other than `trpc` or `api`.
- Query builders, SQL built at run time (`$queryRawUnsafe` with a computed text, a `Prisma.sql` fragment that is no statement), raw SQL on a Prisma client behind a receiver of more than three segments, and Prisma clients under a root other than the chain roots of the `prisma` pack, unless the file reaches the client through an alias of `this` or declares the name with one of the root types of the pack, directly or through a `type` alias of the same file, or the first parameter of a callback given to `$transaction`. A name that the file also binds anywhere else to anything but `this` or a root type is not read (`function a(tx2: PrismaClient)` beside `items.map(tx2 => tx2)` reads no table), and a root type that the file declares as a type of its own is that local type.
- A Prisma root type that a file imports under another name (`PrismaClient as Base`), reaches through a renamed namespace (`P.TransactionClient`), writes as `typeof` a name, or aliases as an import type (`type Db = import('@prisma/client').PrismaClient`).
- The model types of a Prisma client generated to a custom `output` path, or of a package that re-exports the client with `export * from '@prisma/client'`: only the names a file imports from `@prisma/client` itself and uses as a value name a table.
- An enum imported by name from `@prisma/client` and used as a value reads as a table of that name that no model declares, because the import does not say which names are models. A model type that the file only imports, re-exports or writes in a type names no table.
- Environment variables read through an alias of the environment object.

</details>

<details>
<summary>Python</summary>

- GraphQL servers (Strawberry, Graphene) and task queues (Celery, RQ, Dramatiq).
- FastAPI routers and Flask blueprints created in another module than the one that declares their routes.
- Router mounts made from another Python project, in a cycle, deeper than 32 routers or in more than 64 ways, through an import that cannot be resolved, or through a name re-exported by a module other than a package's `__init__.py`.
- FastAPI applications mounted under a path with `mount`, as Starlette does, and routes added with `add_api_route`.
- Flask routes with several methods are read for the first one only, methods held in a variable are read as any method, and `add_url_rule` and class-based views are not read.
- Django `re_path` and regex routes, Django REST framework routers, and Django's default table names.
- HTTP clients other than requests and httpx, sessions and clients created in another module than the one that uses them, and a client library imported under one name in both `try` and `except`, of which the last import is read.
- Names brought in by a star import are not matched to the module they come from.
- SQLAlchemy table names given by `declared_attr` or inherited from a mixin, and DB-API `execute` calls on a receiver of more than three segments.
- Settings read by environs, django-environ or python-decouple, fields of a settings class that derives from another settings class, and an alias written inside a field's annotation.

</details>

<details>
<summary>Rust</summary>

- Names that a glob import brings in, such as `prop` after `use proptest::prelude::*`: a path that starts with one is reported as an unknown package.
- Paths inside `macro_rules!` bodies, items generated by macros, and code included with `include!` from generated files.
- A crate target with more than 10000 modules is read up to that many.
- A module declared twice under different `cfg`s, such as `unix.rs` and `win.rs` behind one `mod sys;`: every path that names it reaches the first file, and none reaches the other.
- actix `configure`: the routes a function registers on a `ServiceConfig` keep the path they are written with.
- Routers built in another crate, re-exported with `pub use`, kept in a field, passed as an argument, returned from a branch or mounted with `nest_service` are not placed under the prefix they are mounted at, and neither are routers built by an associated function (`Type::router()`), nested more than 64 deep, or entered more than 50000 times in one walk.
- A request through a client returned by another function or hidden behind a type alias, and a URL with no literal path.
- tonic gRPC services, Diesel `table!`, and queries written with the Diesel or SeaORM builders.
- *Not reported:* axum 0.7 wildcard paths such as `/*rest`; facade crates such as `shuttle-axum`, because a framework's rules apply only to a dependency the manifest declares itself; a dependency renamed to a framework's name, such as `axum = { package = "my-router" }`, switches that framework's rules on; dev-dependencies are visible to non-test code; the functions a router reaches are matched by name within a file.

</details>

<details>
<summary>Go</summary>

- A method called on a value whose type the file does not show, so a file that holds only such a caller is not listed by `impact --name`.
- Calls through an interface, which are not followed to the types that implement it, and methods promoted from an embedded type.
- Build tags and `GOOS` suffixes: every file of a package counts, whatever its constraint.
- `replace` directives of `go.mod` and `go.work` files: a module is read from its own `go.mod` alone.
- Files that use the Go 1.26 form `new(expr)`: the grammar gsttt reads with does not know it yet, so such a file is read as recovered. It keeps its declarations, exports and imports, but inside the function that holds the form some uses may be lost.
- Routers handed to another function or mounted with `Mount` (their routes keep no prefix), gorilla routes built with `Path(...).Methods(...).HandlerFunc(...)`, clients held in a variable (`client := &http.Client{}; client.Get(url)`), gRPC, and a value defined in another file.

</details>

<details>
<summary>Contracts</summary>

- OpenAPI path items that refer to another place in the same document.
- GraphQL root types renamed by a `schema` definition in another file, `# import` lines, and fragments spread from other files.
- protobuf HTTP rules written in a gRPC service config file.
- Prisma environment variables read outside the `datasource` block.
- SQL tables created by ORM migrations written in code or inside stored procedures, statements the SQL parser does not read, and tables named like a common table expression or a table function elsewhere in the same statement.
- Environment variables set in compose files, CI and deployment manifests.
- A document embedded in code in a language no connector reads.
- *Not reported:* MySQL's `RENAME TABLE a TO b` declares nothing (`ALTER TABLE a RENAME TO b` does), and a gateway prefix with a parameter, such as `/api/{version}`, is taken off only paths that hold a parameter in that place, not `/api/v1/…`.

</details>

<details>
<summary>Indexing and trust (not reported)</summary>

- When git filters a file on checkout (`core.autocrlf`, a `.gitattributes` filter), its facts are keyed by the blob in the git index but read from the bytes in the work tree.
- A save between the listing and the read that keeps both the file's size and its modification time leaves the new facts under the old bytes until the next run.
- A crash between writing the index and renaming it into place leaves a temporary file behind, and so does a crash while `gsttt config trust` writes (`trust.*.tmp` in the state directory).
- The trust key of an external connector covers the files its command line names, not the modules a script imports or the files a binary reads, and not the target of a link inside the repository that points outside it, so a pull that changes only those runs the connector without a new prompt.

</details>

## License

Apache-2.0.
