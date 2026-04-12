FROM rust:1-slim-bookworm AS builder

RUN apt-get update \
    && apt-get install -y musl-tools pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add x86_64-unknown-linux-musl

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
RUN mkdir src \
    && echo "fn main() {}" > src/main.rs \
    && cargo build --release --target x86_64-unknown-linux-musl \
    && rm -rf src

COPY src ./src
COPY tests ./tests

RUN touch src/main.rs \
    && cargo test --release --target x86_64-unknown-linux-musl \
    && cargo build --release --target x86_64-unknown-linux-musl

FROM gcr.io/distroless/static-debian13:latest

COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/emissary /emissary

ENTRYPOINT ["/emissary"]
