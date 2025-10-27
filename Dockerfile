# Three stages:
    # 1. Chef stage: Prepares the environment for caching dependencies
    # 2. Planner/Builder stage: Caches our dependencies and builds the binary(app)
    # 3. Runtime stage: Copies the built binary into a minimal image for running

# https://github.com/LukeMathWalker/cargo-chef is a tool that helps optimize Docker builds for Rust projects by caching dependencies effectively.
FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef

# Set the working directory inside the container
WORKDIR /app

# Install necessary system dependencies
RUN apt update && apt install lld clang -y

FROM chef AS planner

# Copy all project files to the container
COPY . .

# Create a recipe for caching dependencies by computing a lockfile
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder

# Copy the recipe (lockfile) from the planner stage
COPY --from=planner /app/recipe.json recipe.json

# Install the dependencies specified in the recipe
RUN cargo chef cook --release --recipe-path recipe.json

# Copy the entire project source code to the container
COPY . .

# Set environment variable to enable offline mode for SQLx
ENV SQLX_OFFLINE=true

# Build the Rust application in release mode
RUN cargo build --release --bin zero2prod

# Final/Runtime stage (Used in final image size)
FROM debian:bookworm-slim AS runtime

WORKDIR /app

# Install necessary runtime dependencies
# Openssl for TLS support and ca-certificates for secure connections
RUN apt-get update -y\
    && apt-get install -y --no-install-recommends openssl ca-certificates \
    && apt-get autoremove -y \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Copy the compiled binary from the builder stage to the runtime stage
COPY --from=builder /app/target/release/zero2prod zero2prod

# Copy the configuration directory to the runtime stage
COPY configuration configuration

# Set environment variable for application environment
ENV APP_ENVIRONMENT=production
ENV RUST_LOG=trace

# When `docker run` is called, execute the compiled binary
ENTRYPOINT ["./zero2prod"]