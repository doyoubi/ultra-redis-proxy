FROM rust:alpine AS build
RUN apk add --no-cache musl-dev git
RUN git clone https://github.com/doyoubi/ultra-redis-proxy.git
WORKDIR /ultra-redis-proxy
RUN cargo build --release

FROM alpine:latest
COPY --from=build /ultra-redis-proxy/target/release/ultra-redis-proxy /usr/bin/ultra-redis-proxy
