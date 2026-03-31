# ---- Builder ----
FROM rust:1.94-bookworm AS builder

WORKDIR /app

# Cache dependencies: copy only manifests first
COPY Cargo.toml Cargo.lock ./
COPY crates/ox-core/Cargo.toml crates/ox-core/
COPY crates/ox-compiler/Cargo.toml crates/ox-compiler/
COPY crates/ox-runtime/Cargo.toml crates/ox-runtime/
COPY crates/ox-brain/Cargo.toml crates/ox-brain/
COPY crates/ox-source/Cargo.toml crates/ox-source/
COPY crates/ox-store/Cargo.toml crates/ox-store/
COPY crates/ox-api/Cargo.toml crates/ox-api/

# Create dummy source files so cargo can resolve the workspace
RUN for dir in crates/*/; do \
        mkdir -p "$dir/src" && echo "" > "$dir/src/lib.rs"; \
    done && \
    echo "fn main() {}" > crates/ox-api/src/main.rs

# Build dependencies only (this layer is cached unless Cargo.toml/lock change)
RUN cargo build --release --bin ontosyx 2>/dev/null || true

# Copy actual source and rebuild
COPY crates/ crates/
RUN touch crates/*/src/*.rs && cargo build --release --bin ontosyx

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

COPY --from=builder /app/target/release/ontosyx /usr/local/bin/ontosyx
COPY prompts/ /app/prompts/

# Config file is optional (env vars override everything), but copy if present
COPY ontosyx.toml /app/ontosyx.toml

RUN chown -R app:app /app

USER app

EXPOSE 3001

CMD ["ontosyx"]
