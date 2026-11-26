# Build from the repo root: podman build -f relay-server/Dockerfile .
FROM docker.io/library/rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p p2p-relay-server

FROM scratch
COPY --from=build /src/target/release/relay-server /relay-server
USER 65534:65534
EXPOSE 47802/tcp
ENTRYPOINT ["/relay-server"]
