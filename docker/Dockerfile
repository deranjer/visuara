# Build context is the repository root (see docker-compose.yml), since the
# signaling server is part of the Cargo workspace and needs visuara-common.
FROM rust:1-bookworm AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends build-essential \
    && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY visuara-common ./visuara-common
COPY visuara-signaling ./visuara-signaling
COPY visuara-agent ./visuara-agent
COPY visuara-client ./visuara-client
RUN cargo build --release -p visuara-signaling

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/visuara-signaling /usr/local/bin/visuara-signaling

ENV DATABASE_PATH=/data/visuara.db
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/visuara-signaling"]
