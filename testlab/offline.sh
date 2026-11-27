#!/bin/sh
# Entry point of the lab image. Removes the default route, so the container
# can reach the other containers on its own network but nothing on the
# internet: no public DHT, no relay. Then runs the given command.
#
# We do this instead of a Docker network with `internal: true`, because an
# internal network also drops multicast, and then mDNS cannot work.
set -eu

if [ -n "$(ip route show default)" ]; then
    ip route del default || {
        echo "offline.sh: cannot remove the default route; the container needs cap_add: [NET_ADMIN]" >&2
        exit 1
    }
fi
if [ -n "$(ip route show default)" ]; then
    echo "offline.sh: the container still has a default route; refusing to start" >&2
    exit 1
fi

exec "$@"
