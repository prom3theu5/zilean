# Zilean container image.
#
# Phase 4/6 of the .NET -> Rust migration collapsed the build to a single
# Rust stage. There is no .NET SDK, no scraper subprocess, and no legacy
# Go/Python torrent parser in the image; the `zilean` binary serves HTTP,
# runs the scheduler, and performs every one-shot CLI subcommand on its
# own.

FROM --platform=$BUILDPLATFORM rust:1.87-slim AS builder
ARG TARGETOS
ARG TARGETARCH

RUN apt-get update && apt-get install -y \
    perl \
    make \
    cmake \
    pkg-config \
    curl \
    build-essential \
    protobuf-compiler \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

ENV OPENSSL_STATIC=1
ENV OPENSSL_NO_VENDOR=0

WORKDIR /build

# Copy the workspace root plus every member crate. The workspace manifest
# lists src/RustServer and src/ParsettOverToRust as members; the Protos
# directory is referenced from build.rs via a relative path.
COPY Cargo.toml ./Cargo.toml
COPY src/Protos ./src/Protos
COPY src/ParsettOverToRust ./src/ParsettOverToRust
COPY src/RustServer ./src/RustServer

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    case "$TARGETARCH" in \
        amd64) PLATFORM=x86_64 ;; \
        arm64) PLATFORM=aarch64 ;; \
        *) echo "Unexpected TARGETARCH '$TARGETARCH'" >&2; exit 1 ;; \
    esac && \
    rustup target add $PLATFORM-unknown-linux-gnu && \
    export TARGET=$PLATFORM-unknown-linux-gnu && \
    cargo build --release --target=$TARGET -p zilean_rust && \
    cp target/$TARGET/release/zilean_rust /zilean

FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
VOLUME /app/data

COPY --from=builder --chmod=0755 /zilean /app/zilean

# ZILEAN_HTTP_PORT defaults to 8181 in the binary itself (matching the
# port the retired .NET ApiService used); operators who need a different
# port can override it at runtime.
EXPOSE 8181
ENTRYPOINT ["/app/zilean"]
CMD ["serve"]
