FROM rust:1.85 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p datum-sync-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/datum-sync-server /usr/local/bin/
EXPOSE 3000
CMD ["datum-sync-server"]
