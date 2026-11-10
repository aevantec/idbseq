# syntax=docker/dockerfile:1.7
#
# idbseq server image: static musl binaries on distroless, running as non-root.
#
#   docker build -t idbseq .
#   docker run -d -p 7878:7878 -v idbseq-data:/data -e IDBSEQ_ADMIN_PASSWORD=… idbseq
#
# Multi-arch: the build stage runs on the build host and cross-compiles for
# TARGETARCH with cargo-zigbuild, so no emulation is needed.

ARG RUST_VERSION=1.99.1
ARG ZIG_VERSION=0.17.1
ARG CARGO_ZIGBUILD_VERSION=0.24.1

FROM --platform=$BUILDPLATFORM rust:${RUST_VERSION}-slim-bookworm AS build
ARG RUST_VERSION
ARG ZIG_VERSION
ARG CARGO_ZIGBUILD_VERSION
ARG TARGETARCH
ENV RUSTUP_TOOLCHAIN=${RUST_VERSION}

# Debian security updates move package versions; pinning them breaks rebuilds.
# hadolint ignore=DL3008
RUN apt-get update \
    && apt-get install -y --no-install-recommends python3-pip \
    && rm -rf /var/lib/apt/lists/* \
    && pip3 install --no-cache-dir --break-system-packages "ziglang==${ZIG_VERSION}" \
    && cargo install --locked "cargo-zigbuild@${CARGO_ZIGBUILD_VERSION}" \
    && rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl

WORKDIR /src
COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/src/target,id=idbseq-target-${TARGETARCH} \
    case "${TARGETARCH}" in \
      amd64) target=x86_64-unknown-linux-musl ;; \
      arm64) target=aarch64-unknown-linux-musl ;; \
      *) echo "unsupported TARGETARCH ${TARGETARCH}" >&2; exit 1 ;; \
    esac \
    && cargo zigbuild --release --locked --target "${target}" --bin idbseqd --bin idbseq \
    && mkdir -p /out/bin /out/data \
    && cp "target/${target}/release/idbseqd" "target/${target}/release/idbseq" /out/bin/

FROM gcr.io/distroless/static-debian12:nonroot

LABEL org.opencontainers.image.title="idbseq" \
      org.opencontainers.image.description="Increasing numeric ID sequences over HTTP/JSON" \
      org.opencontainers.image.source="https://github.com/aevantec/idbseq"

COPY --from=build /out/bin/idbseqd /out/bin/idbseq /usr/local/bin/
# The volume is initialised from this directory, so it is writable by nonroot.
COPY --from=build --chown=65532:65532 /out/data /data

ENV IDBSEQ_DATA_DIR=/data \
    IDBSEQ_BIND=0.0.0.0:7878

USER 65532:65532
VOLUME /data
EXPOSE 7878

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/usr/local/bin/idbseqd", "healthcheck"]

ENTRYPOINT ["/usr/local/bin/idbseqd"]
