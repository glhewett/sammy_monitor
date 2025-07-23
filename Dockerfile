# Multi-stage build for smaller image size
FROM rust:1.75-alpine as builder

WORKDIR /app

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    openssl-dev \
    openssl-libs-static \
    pkgconfig

# Set environment variables for static linking
ENV RUSTFLAGS="-C target-feature=+crt-static"

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src
COPY templates ./templates

# Build the application with static linking
RUN cargo build --release --target x86_64-unknown-linux-musl

# Runtime stage - use minimal Alpine base
FROM alpine:latest

# Install minimal runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    curl

# Create a non-root user
RUN adduser -D -s /bin/sh sammy

WORKDIR /app

# Copy the statically linked binary
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/sammy_monitor /usr/local/bin/sammy_monitor

# Copy templates
COPY --from=builder /app/templates ./templates

# Copy sample settings file
COPY settings.sample.toml ./settings.sample.toml

# Change ownership to sammy user
RUN chown -R sammy:sammy /app

# Switch to non-root user
USER sammy

# Expose ports
EXPOSE 3000 3001

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1

# Default command
CMD ["sammy_monitor", "--settings", "/app/settings.toml"]
