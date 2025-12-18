FROM rust:alpine AS builder

# Install build dependencies
RUN apk add --no-cache protoc musl-dev

WORKDIR /app

# Copy proto files
COPY proto /app/proto

# Copy rust project
COPY server /app/server

WORKDIR /app/server

# Build
RUN cargo build --release

# Export stage
FROM scratch AS export
COPY --from=builder /app/server/target/release/mikaboshi-server /mikaboshi-server
