# syntax=docker/dockerfile:1

FROM rust:1.95-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim AS runtime

RUN useradd --create-home --shell /usr/sbin/nologin shadowboy \
    && apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/sb-netplay-serv /usr/local/bin/sb-netplay-serv

USER shadowboy
EXPOSE 8077
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["curl", "-fsS", "http://127.0.0.1:8077/health"]

ENTRYPOINT ["/usr/local/bin/sb-netplay-serv"]
