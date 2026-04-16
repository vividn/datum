FROM rust:1.76 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p datum-sync

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/datum-sync /usr/local/bin/
CMD ["datum-sync"]
