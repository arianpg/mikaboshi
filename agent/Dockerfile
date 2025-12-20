FROM rust:alpine AS builder

# Install build dependencies
RUN apk add --no-cache protoc musl-dev libpcap-dev git

WORKDIR /app

# Copy proto files
COPY proto /app/proto

# Copy rust project
COPY agent /app/agent

WORKDIR /app/agent

# Build
# We accept that this might re-download crates every time unless we cache dependencies
# Optimization: Copy Cargo files first, build dummy, then copy src. 
# But for now, simple build.
RUN cargo build --release

# Export stage
FROM scratch AS export
COPY --from=builder /app/agent/target/release/mikaboshi-agent /mikaboshi-agent
COPY agent/THIRD_PARTY_LICENSES /THIRD_PARTY_LICENSES
