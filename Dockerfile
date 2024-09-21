FROM rust:1.81-alpine AS builder
WORKDIR /usr/src/movement-tracker

# Install dependencies needed for building
RUN apk add --no-cache --purge openssl-dev openssl-libs-static musl-dev libc-dev

# Copy sources
COPY src ./src
COPY Cargo.toml Cargo.lock ./
# Offline query cache for sqlx build, generate with `cargo sqlx prepare`
COPY .sqlx ./.sqlx

# Build the project checking against the actual database
RUN cargo install --path .

FROM alpine AS runtime
RUN apk add --no-cache openssl
COPY --from=builder /usr/local/cargo/bin/movement-tracker /usr/local/bin/movement-tracker
# Expose the health check port
EXPOSE 8080
CMD ["movement-tracker"]
