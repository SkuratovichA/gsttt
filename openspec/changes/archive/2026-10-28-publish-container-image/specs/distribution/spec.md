# Spec Delta

## ADDED Requirements

### Requirement: Container image
After a release, the `publish` job SHALL push a container image of `gsttt` for `linux/amd64` and `linux/arm64` to `ghcr.io/<owner>/gsttt`, tagged with the version of the release and with `latest` only when the release is the latest. The image SHALL hold the binaries of that release's Linux packages, unpacked after their SHA-256 matched, and a version whose image exists SHALL get no second one.

#### Scenario: The image is built from the checked packages
- **WHEN** the step that publishes the image runs for a release whose Linux packages are in `packages/`
- **THEN** it checks both archives against their `.sha256` files, builds from a context that holds only the `gsttt` of each, and pushes one image for `linux/amd64` and `linux/arm64` tagged `ghcr.io/<owner>/gsttt:<version>`, having given the registry the token on standard input only

#### Scenario: A package that does not match its checksum is not published
- **WHEN** a Linux archive does not match its `.sha256` file
- **THEN** the step fails and builds and pushes nothing

#### Scenario: Only the highest version is the latest image
- **WHEN** the step runs for a version below an existing release
- **THEN** the image is tagged with its version and not with `latest`

#### Scenario: A version with an image is not pushed again
- **WHEN** `ghcr.io/<owner>/gsttt:<version>` exists
- **THEN** the step builds and pushes nothing

#### Scenario: A run after the release was made still publishes the image
- **WHEN** the tag of the version already points at the commit and no image has that version
- **THEN** the step pushes the image

### Requirement: Image contents
The image SHALL hold the `gsttt` binary and nothing else, on a `gcr.io/distroless/static` base, with `/repo` as its working directory and `gsttt` as its entry point. The base and the BuildKit that builds the image SHALL each be pinned by the digest of its index, and the OCI labels `source`, `description`, `licenses`, `version` and `revision` SHALL be repeated as annotations of the index, which is where GitHub reads them for a multi-platform image.

#### Scenario: The image holds only the binary
- **WHEN** the `Dockerfile` is read
- **THEN** it adds the `gsttt` of the target architecture to a distroless static base, runs no command, sets `/repo` as the working directory and makes `gsttt` the entry point

#### Scenario: The index repeats the labels
- **WHEN** the `Dockerfile` and the publish step are read
- **THEN** the step annotates the index with the key of every label of the `Dockerfile`, and with the same text for each label that is fixed text

#### Scenario: The base and the builder are pinned by digest
- **WHEN** the `Dockerfile` and the publish step are read
- **THEN** the base image and the BuildKit image are each named with a tag and a `sha256` digest
