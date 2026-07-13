# syntax=docker/dockerfile:1.7

# ---- Cargo Chef Base ----
FROM rust:1.97-bookworm AS chef

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    libprotobuf-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo install cargo-chef --locked

# ---- Dependency Planner ----
FROM chef AS planner

COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo chef prepare --recipe-path recipe.json

# ---- Builder ----
FROM chef AS builder

COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo chef cook --profile container --bin ontosyx --features source-all --recipe-path recipe.json

COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --profile container --bin ontosyx --features source-all && \
    cp /app/target/container/ontosyx /usr/local/bin/ontosyx

# ---- Runtime ----
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Non-root user
RUN groupadd --gid 1000 app && \
    useradd --uid 1000 --gid app --create-home app

WORKDIR /app

COPY --from=builder /usr/local/bin/ontosyx /usr/local/bin/ontosyx
COPY prompts/ /app/prompts/

# Config file is optional (env vars override everything), but copy if present
COPY ontosyx.toml /app/ontosyx.toml

RUN chown -R app:app /app

USER app

EXPOSE 3101

CMD ["ontosyx"]
