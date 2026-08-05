# syntax=docker/dockerfile:1

FROM rust:1.96.0-slim-bookworm AS build
WORKDIR /source
COPY . .
RUN cargo build --locked --release --package poche-web-spike

FROM debian:bookworm-slim AS runtime
RUN useradd --system --no-create-home --uid 10001 poche
COPY --from=build /source/target/release/poche-web-spike /usr/local/bin/poche-web-spike
ENV POCHE_WEB_SPIKE_ADDR=0.0.0.0:4174
EXPOSE 4174
USER 10001:10001
ENTRYPOINT ["/usr/local/bin/poche-web-spike"]
