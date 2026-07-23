FROM rust:slim

WORKDIR /app

# Copy manifests first for better layer caching
COPY project/slopario/backend/Cargo.toml project/slopario/backend/Cargo.lock ./
COPY project/slopario/backend/src ./src

# Build the app
RUN cargo build --release

EXPOSE 6969

CMD ["cargo", "run", "--release"]
