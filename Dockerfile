FROM ekidd/rust-musl-builder:nightly-2020-08-15 AS builder
ENV CARGO_TERM_COLOR="always"
RUN sudo chown -R rust:rust /home/rust
RUN rustup target add x86_64-unknown-linux-musl
COPY . .
RUN cargo build --release

FROM alpine:latest
COPY --from=builder \
    /home/rust/src/target/x86_64-unknown-linux-musl/release/sticker_eater \
    /usr/local/bin/
WORKDIR /sticker_eater
ENV TELEGRAM_BOT_TOKEN
ENTRYPOINT /usr/local/bin/sticker_eater