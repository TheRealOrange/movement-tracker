FROM rust:1.81-alpine AS builder
WORKDIR /usr/src/movement-tracker

# Install dependencies needed for building
RUN apk add --no-cache --purge openssl-dev openssl-libs-static musl-dev libc-dev

# Copy sources
COPY src ./src
COPY Cargo.toml Cargo.lock ./
# Offline query cache for sqlx build, generate with `cargo sqlx prepare`
COPY .sqlx ./.sqlx
# Migrations directory
COPY ./migrations ./migrations

# Build the project checking against the actual database
RUN cargo install --path .

FROM alpine AS runtime
RUN apk add --no-cache openssl curl
# App directory
WORKDIR /app
COPY ./migrations /app/migrations
COPY --from=builder /usr/local/cargo/bin/movement-tracker /app/movement-tracker
# Expose the health check port
EXPOSE 8080
CMD ["/app/movement-tracker"]
