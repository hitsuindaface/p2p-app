# Build from the repo root: podman build -f directory-server/Dockerfile .
FROM docker.io/library/rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p p2p-directory-server

FROM scratch
COPY --from=build /src/target/release/directory-server /directory-server
USER 65534:65534
EXPOSE 47802/udp
ENTRYPOINT ["/directory-server"]
