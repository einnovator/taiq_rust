# syntax=docker/dockerfile:1.7
FROM rust:1-bookworm AS builder
RUN apt-get update && apt-get install -y --no-install-recommends cmake clang pkg-config \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --locked --release --no-default-features --bin taiq-agent

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir /workspace \
    && chown 10001:10001 /workspace
COPY --from=builder /build/target/release/taiq-agent /usr/local/bin/taiq-agent
USER 10001:10001
WORKDIR /workspace
ENV HOME=/workspace TAIQ_CONFIG=/forgeiq/config/config.yaml
ENTRYPOINT ["/usr/local/bin/taiq-agent"]
