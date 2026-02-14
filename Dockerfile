FROM rust:1.93-alpine AS planner
RUN cargo install cargo-chef
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY templates/ templates/
RUN cargo chef prepare --recipe-path recipe.json

FROM rust:1.93-alpine AS builder
RUN apk add --no-cache musl-dev
RUN cargo install cargo-chef
WORKDIR /build
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --target x86_64-unknown-linux-musl --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY templates/ templates/
RUN cargo build --release --target x86_64-unknown-linux-musl

FROM gcr.io/distroless/static:nonroot
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/config-reload-demo /
EXPOSE 8080
ENTRYPOINT ["/config-reload-demo"]
