FROM rust:alpine as builder
WORKDIR /home/rust/src
RUN apk --no-cache add musl-dev
COPY . .
RUN cargo install --path .

FROM scratch
COPY --from=builder /usr/local/cargo/bin/conduktor-kafka-proxy .
USER 1000:1000
ENTRYPOINT ["./conduktor-kafka-proxy"]
