# Packs the release binary CI built: the build context holds amd64/gsttt and arm64/gsttt, so `docker build .` from a checkout is not expected to work.
FROM gcr.io/distroless/static-debian13:latest@sha256:15f8637509792e05b97d83451b0b6ccfa4abd548f997e406ac1e72c87fe1fa78
ARG TARGETARCH
ARG VERSION
ARG REVISION
LABEL org.opencontainers.image.source="https://github.com/SkuratovichA/gsttt"
LABEL org.opencontainers.image.description="What a code change affects, across files, packages and languages"
LABEL org.opencontainers.image.licenses="Apache-2.0"
LABEL org.opencontainers.image.version="${VERSION}"
LABEL org.opencontainers.image.revision="${REVISION}"
COPY --chmod=0755 ${TARGETARCH}/gsttt /usr/local/bin/gsttt
# The repository is mounted here, and gsttt works on the one around the directory it starts in.
WORKDIR /repo
ENTRYPOINT ["/usr/local/bin/gsttt"]
