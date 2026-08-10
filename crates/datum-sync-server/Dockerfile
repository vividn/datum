# The Rust version is not decided here. `rust-toolchain.toml` at the repo root
# is the single source, and rustup obeys it — so this image compiles with the
# pinned toolchain regardless of the tag below. The tag is a cache hint: when it
# matches the pin, the base image's preinstalled compiler is the one we build
# with, instead of a second one fetched at build time. It read `1.85` long after
# the workspace had moved on, which is what an unchecked second copy of a
# version does; `cargo xtask check toolchain-pin` now fails if the two disagree.
FROM rust:1.97.1 AS builder
WORKDIR /app

# Toolchain acquisition gets its own layer, keyed on the pin alone, so editing
# source never re-runs it. Bare `rustup toolchain install` takes the channel,
# components and targets from rust-toolchain.toml — the invocation CI uses too.
COPY rust-toolchain.toml ./
RUN rustup toolchain install --no-self-update

COPY . .
RUN cargo build --release -p datum-sync-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/datum-sync-server /usr/local/bin/
EXPOSE 3000
CMD ["datum-sync-server"]
