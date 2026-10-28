# Tasks

## 1. Image

- [x] 1.1 `Dockerfile`: the binary of the build context on a distroless static base pinned by the digest of its index, the OCI labels from build arguments and text, `/repo` as the working directory
- [x] 1.2 The `publish` job gets `packages: write` and one step after `Release`: log in with the token on standard input, skip a version that has an image, check the Linux packages, and push one image for both platforms from a pinned BuildKit
- [x] 1.3 The README shows how to run the image

## 2. Proof

- [x] 2.1 A test annotated `#[scenario("distribution", …)]` for every scenario of this change's delta, the step run against a `docker` that writes its calls down
- [x] 2.2 `cargo nextest run --workspace`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, `taplo fmt --check`, `cargo xtask spec`, `cargo xtask check` and `openspec validate publish-container-image --strict` pass
- [x] 2.3 The image built from the `Dockerfile` and the arm64 package of a release answers `--version` and `impact`, and the MCP server in it answers `initialize`
