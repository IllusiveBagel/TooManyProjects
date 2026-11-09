# syntax=docker/dockerfile:1

# ---- 1. Web app -> static files ------------------------------------------------
FROM node:22-alpine AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY themes /src/themes
COPY web ./
RUN npm run build

# ---- 2. Rust server (cargo-chef caches dependencies between builds) ----------
FROM rust:1-slim-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /src/server

FROM chef AS planner
COPY server ./
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS server
COPY --from=planner /src/server/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY themes /src/themes
COPY server ./
# The release workflow passes the tag (such as 1.2.0). The app compares it with
# the latest GitHub release to tell the owner about updates; without it the
# build counts as a development build and never shows the update banner.
ARG VERSION=""
RUN TMP_BUILD_VERSION="$VERSION" cargo build --release --locked \
 && mkdir -p /out/data \
 && cp target/release/toomanyprojects /out/

# ---- 3. Runtime: one binary, the static files, nothing else -------------------
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=server /out/toomanyprojects /app/toomanyprojects
COPY --from=web /src/web/dist /app/web
# Owned by the nonroot user so a fresh named volume is writable.
COPY --from=server --chown=65532:65532 /out/data /data
ENV TMP_DATA_DIR=/data \
    TMP_STATIC_DIR=/app/web \
    TMP_BIND=0.0.0.0:8080
VOLUME /data
EXPOSE 8080 8443
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s CMD ["/app/toomanyprojects", "healthcheck"]
ENTRYPOINT ["/app/toomanyprojects"]
CMD ["serve"]
