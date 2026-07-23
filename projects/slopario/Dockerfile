FROM rust:slim

WORKDIR /app

# Copy manifests first for better layer caching
COPY projects/slopario/backend/Cargo.toml projects/slopario/backend/Cargo.lock ./
COPY projects/slopario/backend/src ./src

# Build the app
RUN cargo build --release

EXPOSE 6969

CMD ["cargo", "run", "--release"]
