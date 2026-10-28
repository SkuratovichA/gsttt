# gsttt

**Glass sphere telling the truth**, said "ghost". It answers one question without guessing: what does this change affect?

gsttt builds a graph of the files in a repository and of the contracts between them: imports, HTTP routes and the calls that reach them, GraphQL, gRPC and tRPC operations, database tables and environment variables. It follows them across packages, languages and services. No model runs anywhere in it, so the same tree always gives the same answer, and every answer says how much of the repository it covers and how fresh the index is.

## Status

Under construction. The first release covers TypeScript, Python and Rust, and names what it cannot see yet instead of guessing.

## Modules

- Languages: TypeScript and JavaScript, Python, Rust.
- Contracts: OpenAPI, GraphQL, protobuf, Prisma, SQL, `.env` files.
- Framework knowledge lives in Lua rule packs, and any other language can join as an external connector that speaks JSON lines.
- Configuration is Lua, in `.gsttt/init.lua`, with LuaLS types for the editor.

## Surfaces

- `gsttt` on the command line.
- `gsttt mcp`, an MCP server, shipped as a Claude Code plugin from this repository.

## License

Apache-2.0.
