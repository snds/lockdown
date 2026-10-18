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
# Behind a proxy that re-terminates TLS, pass its CA as a build secret:
#   docker build --secret id=cargo_ca,src=/path/to/ca.pem .

ARG RUST_VERSION=1.97

FROM rust:${RUST_VERSION}-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN --mount=type=secret,id=cargo_ca,required=false \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    if [ -f /run/secrets/cargo_ca ]; then export CARGO_HTTP_CAINFO=/run/secrets/cargo_ca; fi; \
    cargo build --locked --release -p lockdown-server \
    && install -D -m 0755 target/release/lockdown-server /out/lockdown-server \
    && mkdir -p /out/data

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /out/lockdown-server /usr/local/bin/lockdown-server
COPY --from=build --chown=65532:65532 /out/data /data
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
