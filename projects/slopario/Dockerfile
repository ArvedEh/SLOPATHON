FROM rust:slim

WORKDIR /app

# Copy manifests first for better layer caching
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/src ./src

# Build the app
RUN cargo build --release

EXPOSE 6969

CMD ["cargo", "run", "--release"]
