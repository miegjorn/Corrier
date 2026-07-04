# `rust:latest`, not a pinned version: a transitive dependency (zeroize
# v1.9.0) requires Cargo's edition2024 feature, which rust:1.82 (unstabilized
# as of that release) rejects with "failed to parse manifest ... feature
# `edition2024` is required" -- confirmed live in CI, 2026-07-04. Caissa's own
# Dockerfile already uses `rust:latest` for the same reason (transitive
# dependencies moving faster than a pinned version can track); matching that
# convention here instead of re-pinning to a newer fixed version that will
# just go stale again.
FROM rust:latest AS builder
ARG BIN
WORKDIR /build
COPY . .
# The `nervi` build context is supplied via `docker buildx build --build-context
# nervi=../nervi` (see .github/workflows/build.yml). That context's root *is* the
# nervi repo checkout, so `.` here copies its full tree — giving /nervi/nervi-core,
# which is exactly what corrier-core's `path = "../../nervi/nervi-core"` dependency
# resolves to from /build/corrier-core.
COPY --from=nervi . /nervi
RUN cargo build --release -p ${BIN}

FROM debian:bookworm-slim
# Both binaries link reqwest against the system's OpenSSL at runtime (not
# vendored/rustls) -- confirmed live: omitting this produced "error while
# loading shared libraries: libssl.so.3: cannot open shared object file" on
# every pod, CrashLoopBackOff. Matches Caissa's own Dockerfile, which already
# installs the same two packages for the same reason.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*
ARG BIN
# ARG values do not persist into the running container, but the ENTRYPOINT below
# needs $BIN at container start, not just at build time. Re-exporting it as ENV
# keeps it available at runtime.
ENV BIN=${BIN}
COPY --from=builder /build/target/release/${BIN} /usr/local/bin/${BIN}
ENTRYPOINT ["/bin/sh", "-c", "exec /usr/local/bin/$BIN"]
