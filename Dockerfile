# Multi-stage build untuk optimasi ukuran  
FROM rustlang/rust:nightly-slim as builder

WORKDIR /app

# Install dependencies untuk build
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy dependency files
COPY Cargo.toml ./

# Create dummy main.rs untuk cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Enable unstable features untuk edition2024
ENV RUSTFLAGS="-Z unstable-options"

# Build dependencies (akan di-cache) - Cargo akan generate Cargo.lock otomatis
RUN cargo +nightly build --release && rm -rf src

# Copy source code
COPY src ./src
COPY templates ./templates
COPY migrations ./migrations

# Build aplikasi
RUN touch src/main.rs && cargo +nightly build --release

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy binary dari builder stage
COPY --from=builder /app/target/release/my-porto /app/
COPY --from=builder /app/templates /app/templates

# Copy data directory if exists
COPY ./data /app/data

# Create directories with proper permissions
RUN mkdir -p uploads static && \
    chmod 755 uploads static data

# Create non-root user
RUN useradd -r -s /bin/false appuser && \
    chown -R appuser:appuser /app

USER appuser

EXPOSE 3000

CMD ["./my-porto"]
