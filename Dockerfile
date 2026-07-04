FROM rust:1.82 AS builder
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
ARG BIN
# ARG values do not persist into the running container, but the ENTRYPOINT below
# needs $BIN at container start, not just at build time. Re-exporting it as ENV
# keeps it available at runtime.
ENV BIN=${BIN}
COPY --from=builder /build/target/release/${BIN} /usr/local/bin/${BIN}
ENTRYPOINT ["/bin/sh", "-c", "exec /usr/local/bin/$BIN"]
