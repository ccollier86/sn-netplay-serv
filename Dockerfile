# syntax=docker/dockerfile:1

FROM rust:1.95-bookworm AS builder

ARG SB_NETPLAY_BUILD_SHA=unknown
ARG SB_NETPLAY_IMAGE_IDENTITY=local
ENV SB_NETPLAY_BUILD_SHA=${SB_NETPLAY_BUILD_SHA}
ENV SB_NETPLAY_IMAGE_IDENTITY=${SB_NETPLAY_IMAGE_IDENTITY}

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim AS runtime

ARG SB_NETPLAY_BUILD_SHA=unknown
ARG SB_NETPLAY_IMAGE_IDENTITY=local
ENV SB_NETPLAY_BUILD_SHA=${SB_NETPLAY_BUILD_SHA}
ENV SB_NETPLAY_IMAGE_IDENTITY=${SB_NETPLAY_IMAGE_IDENTITY}

LABEL org.opencontainers.image.source="https://github.com/ccollier86/sn-netplay-serv" \
      org.opencontainers.image.revision="${SB_NETPLAY_BUILD_SHA}" \
      org.opencontainers.image.ref.name="${SB_NETPLAY_IMAGE_IDENTITY}"

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
