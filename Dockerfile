# Multi-stage build untuk optimasi ukuran  
FROM debian:bookworm-slim as builder

WORKDIR /app

# Install dependencies untuk build dan Rust
RUN apt-get update && apt-get install -y \
    curl \
    pkg-config \
    libssl-dev \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Install Rust 1.88.0 menggunakan rustup
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    --default-toolchain 1.88.0 \
    --profile minimal

# Add Rust to PATH
ENV PATH="/root/.cargo/bin:${PATH}"

# Verify Rust installation
RUN rustc --version && cargo --version

# Copy dependency files
COPY Cargo.toml ./

# Create dummy main.rs untuk cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies (akan di-cache) - Cargo akan generate Cargo.lock otomatis
RUN cargo build --release && rm -rf src

# Copy source code
COPY src ./src
COPY templates ./templates

# Build aplikasi
RUN touch src/main.rs && cargo build --release

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

# Create directories with full permissions
RUN mkdir -p uploads static && \
    chmod 777 uploads static

# Run as root to avoid permission issues in container
EXPOSE 3000

CMD ["./my-porto"]
