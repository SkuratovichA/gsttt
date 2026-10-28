<p align="center">
  <img src="assets/gsttt.svg" width="128" height="128" alt="An 8-bit ghost, the gsttt logo">
</p>

# gsttt

It answers one question without guessing: what does this change affect?

gsttt builds a graph of the files in a repository and of the contracts between them: imports, HTTP routes and the calls that reach them, GraphQL, gRPC and tRPC operations, database tables and environment variables. It follows them across packages, languages and services. No model runs anywhere in it, so the same tree always gives the same answer, and every answer says how much of the repository it covers and how fresh the index is.

## Status

Under construction. The first release covers TypeScript, Python and Rust, and names what it cannot see yet instead of guessing.

## Modules

- Languages: TypeScript and JavaScript, Python, Rust.
- Contracts: OpenAPI, GraphQL, protobuf, Prisma, SQL, `.env` files.
- Framework knowledge lives in TOML rule packs, and any other language can join as an external connector that speaks JSON lines.
- Configuration is TOML, in `.gsttt/config.toml`, with a JSON Schema that checks it in the editor.

## Surfaces

- `gsttt` on the command line.
- `gsttt mcp`, an MCP server, shipped as a Claude Code plugin from this repository.

## License

Apache-2.0.
