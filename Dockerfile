# Build FluctlightDB server binary
FROM rust:1.88-bookworm AS builder
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p fluctlight-cli

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tini \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/target/release/fluctlight /usr/local/bin/fluctlight
COPY docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

ENV FLUCTLIGHT_STORAGE=v4 \
    FLUCTLIGHT_HOME=/data \
    FLUCTLIGHT_SERVE_ADDR=0.0.0.0:8792

VOLUME ["/data"]
EXPOSE 8792

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/entrypoint.sh"]
