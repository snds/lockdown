# syntax=docker/dockerfile:1.7
#
# The Lockdown server as one small container (single-container;
# docs/self-host.md, docs/plan/20-self-host-server.md §7).
#
# - Built from this commit with the lock file (`--locked`), release profile.
# - Runs as the distroless `nonroot` user (65532), with nothing but the
#   binary and the C runtime it links: no shell, no package manager.
# - Made for a read-only root file system: everything it writes goes to the
#   one volume, /data (the database, the server key file, attachment
#   files). Give it a tmpfs at /tmp (compose.yaml does).
# - TLS on the server itself: mount a certificate and key and set
#   LOCKDOWN_TLS_CERT and LOCKDOWN_TLS_KEY. Without them it refuses to
#   listen on 0.0.0.0 unless LOCKDOWN_INSECURE_HTTP=1 says a reverse proxy
#   in front does TLS.
#
# - The web vault: build it first (apps/web: pnpm build:wasm && pnpm build)
#   and the image carries apps/web/dist at /usr/share/lockdown/web; serve it
#   with LOCKDOWN_WEB_VAULT=/usr/share/lockdown/web. Without a build there,
#   the folder is empty and the server serves no web vault.
#
# Behind a proxy that re-terminates TLS, pass its CA as a build secret:
#   docker build --secret id=cargo_ca,src=/path/to/ca.pem .

# Reproducible (docs/plan/24-release.md §5): both base images are pinned by
# digest, the compiler is the image's own Rust release (RUSTUP_TOOLCHAIN;
# rust-toolchain.toml's "stable" would fetch whatever is newest), paths are
# remapped out of the binary, and with
# `docker buildx build --build-arg SOURCE_DATE_EPOCH=<commit time>
# --output type=oci,...,rewrite-timestamp=true` two builds of one commit
# are the same bytes (packaging/repro/build-image.sh; CI checks it).
ARG RUST_VERSION=1.97
ARG RUST_DIGEST=@sha256:2775a09d208ff0d7c1f50490c45b62db929e87ba1dcbc3f2132ac71a704bcdd3
ARG RUNTIME_DIGEST=@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f

FROM rust:${RUST_VERSION}-slim-bookworm${RUST_DIGEST} AS build
WORKDIR /src
COPY . .
RUN --mount=type=secret,id=cargo_ca,required=false \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    if [ -f /run/secrets/cargo_ca ]; then export CARGO_HTTP_CAINFO=/run/secrets/cargo_ca; fi; \
    RUSTUP_TOOLCHAIN="$RUST_VERSION" \
    RUSTFLAGS="--remap-path-prefix=/src=/lockdown --remap-path-prefix=/usr/local/cargo=/cargo" \
    cargo build --locked --release -p lockdown-server \
    && install -D -m 0755 target/release/lockdown-server /out/lockdown-server \
    && mkdir -p /out/data /out/web \
    && if [ -f apps/web/dist/index.html ]; then cp -r apps/web/dist/. /out/web/; fi

FROM gcr.io/distroless/cc-debian12:nonroot${RUNTIME_DIGEST}
COPY --from=build /out/lockdown-server /usr/local/bin/lockdown-server
COPY --from=build --chown=65532:65532 /out/data /data
COPY --from=build /out/web /usr/share/lockdown/web
ENV LOCKDOWN_DATA=/data \
    LOCKDOWN_LISTEN=0.0.0.0:8443 \
    LOCKDOWN_SIGNUPS=closed \
    TMPDIR=/tmp \
    SQLITE_TMPDIR=/tmp
USER 65532:65532
VOLUME ["/data"]
EXPOSE 8443
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/usr/local/bin/lockdown-server", "healthcheck"]
ENTRYPOINT ["/usr/local/bin/lockdown-server"]
