# The p2p-app command-line chat for the test lab.
# Build from the repo root: docker build -f testlab/Dockerfile .
# testlab/compose.yaml does this for you.

FROM docker.io/library/rust:1-trixie AS build
WORKDIR /src
# The same system packages a developer installs, read from the [apt] section of
# .dependencies. Tauri needs its GTK packages to compile, even though the CLI
# does not use Tauri yet.
COPY .dependencies .
RUN apt-get update \
 && awk '/^\[/ { on = ($1 == "[apt]"); next } on { sub(/#.*/, ""); if (NF) print $1 }' .dependencies \
    | xargs apt-get install -y --no-install-recommends \
 && rm -rf /var/lib/apt/lists/*
COPY . .
# The cache mounts keep downloaded crates and compiled dependencies between
# local builds. They are not part of the image, so copy the binary out.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p p2p-app --bin p2p-app \
 && cp target/release/p2p-app /usr/local/bin/p2p-app

FROM docker.io/library/debian:trixie-slim
# iproute2 gives `tc`, which the lab uses to slow down or break the network.
RUN apt-get update \
 && apt-get install -y --no-install-recommends iproute2 \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /usr/local/bin/p2p-app /usr/local/bin/p2p-app
COPY testlab/offline.sh /usr/local/bin/offline.sh
# The lab runs as root: `ip route` and `tc` need NET_ADMIN, and Docker gives
# added capabilities only to root inside the container.
ENTRYPOINT ["sh", "/usr/local/bin/offline.sh"]
CMD ["p2p-app"]
