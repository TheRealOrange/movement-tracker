FROM rust:1.81-alpine AS builder
WORKDIR /usr/src/movement-tracker

# Install dependencies needed for building
RUN apk add --no-cache --purge openssl-dev openssl-libs-static musl-dev libc-dev

# Copy source files
COPY movement_tracker ./movement_tracker
COPY libs ./libs
COPY Cargo.toml Cargo.lock ./
# Offline query cache for sqlx build, generate with `cargo sqlx prepare`
COPY .sqlx ./.sqlx

# Build the project checking against the actual database
RUN cargo build --release

FROM alpine AS runtime
RUN apk add --no-cache openssl curl
# App directory
WORKDIR /app
COPY ./movement_tracker/migrations /app/migrations
COPY --from=builder /usr/src/movement-tracker/target/release/movement_tracker /app/movement_tracker
# Expose the health check port
EXPOSE 8080
CMD ["/app/movement_tracker"]
