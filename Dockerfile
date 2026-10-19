# Wistjet in one small image: the web app is built, then embedded in the Rust binary, which
# runs as a non-root user with all data in the /data volume.
#
#   docker build -t wistjet .
#   docker run -d -p 8080:8080 -v wistjet-data:/data wistjet

FROM node:22-bookworm-slim AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY web/ ./
RUN npm run build

FROM rust:1-slim-bookworm AS app
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
COPY --from=web /src/web/dist ./web/dist
# The target cache is locked for the whole step: a multi-arch build must not let one
# architecture's binary overwrite the other's between building and copying it out.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target,sharing=locked \
    cargo build --release --locked && cp target/release/wistjet /wistjet
# An empty folder for the volume, owned by the user the app runs as, so a new volume is writable.
RUN mkdir /data

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=app /wistjet /usr/local/bin/wistjet
COPY --from=app --chown=nonroot:nonroot /data /data
ENV WISTJET_DATA_DIR=/data \
    WISTJET_PORT=8080 \
    WISTJET_OPEN_BROWSER=0
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/wistjet"]
