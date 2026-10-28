# Proposal

## Why

The repository page says "No packages published", and a CI pipeline or an MCP client that starts its servers through `docker run` has no way to use gsttt without running the installer. The release already builds static musl binaries for both Linux architectures, so an image only has to pack them, and nothing is compiled in Docker.

## What Changes

- The `publish` job of `.github/workflows/ci.yml` pushes a multi-platform image (`linux/amd64`, `linux/arm64`) to `ghcr.io/<owner>/gsttt` after each release, from the checked packages of that release, tagged with its version, and with `latest` only for the latest release. A version that has an image gets no second one.
- A `Dockerfile` packs the binary onto `gcr.io/distroless/static-debian13`, pinned by the digest of its index, with the OCI labels. The build context is a directory of unpacked binaries, never the checkout.
- The README shows the two `docker run` commands: the CLI, and the MCP server.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `distribution`: a release also publishes a container image.

## Impact

`Dockerfile`, `.github/workflows/ci.yml` (the `publish` job gets `packages: write` and one step after `Release`), `README.md` and `xtask/tests/dist/`. The step compiles nothing, so the token still reaches no step that compiles code. The first release after this change creates the package, and its visibility is the owner's to set on GitHub.
