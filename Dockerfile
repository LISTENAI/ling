FROM rust:1-bookworm AS dev

RUN rustup toolchain install 1.95.0 --profile minimal --component rustfmt --component clippy \
    && rustup default 1.95.0

WORKDIR /workspace
