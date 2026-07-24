# Build context is the repository root (see docker-compose.yml), since the
# signaling server is part of the Cargo workspace and needs visuara-common.

# The web UI (web/dist) is embedded into the signaling binary at compile
# time via rust-embed (see visuara-signaling/src/assets.rs), so it has to
# exist before `cargo build` runs.
FROM node:22-bookworm AS frontend-builder
WORKDIR /app/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

FROM rust:1-bookworm AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends build-essential \
    && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY visuara-common ./visuara-common
COPY visuara-signaling ./visuara-signaling
COPY visuara-agent ./visuara-agent
COPY visuara-client ./visuara-client
COPY --from=frontend-builder /app/web/dist ./web/dist
RUN cargo build --release -p visuara-signaling

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/visuara-signaling /usr/local/bin/visuara-signaling

ENV DATABASE_PATH=/data/visuara.db
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/visuara-signaling"]
