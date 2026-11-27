#!/usr/bin/env bash
# Smoke test for the test lab: two fresh nodes find each other on the lab
# network and alice sends bob a message. Also checks whether tc netem works.
#
# Run from anywhere: bash testlab/smoke.sh
# Logs are saved in testlab/results/. Exits non-zero when the test fails.
set -euo pipefail

cd "$(dirname "$0")"
results=results
mkdir -p "$results"
compose=(docker compose -f compose.yaml)

cleanup() {
    "${compose[@]}" logs --no-color --no-log-prefix bob >"$results/bob.log" 2>&1 || true
    "${compose[@]}" down --volumes --timeout 5 >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "Building the node image..."
"${compose[@]}" build

echo "Starting bob..."
"${compose[@]}" up -d bob

# offline.sh refuses to start the app with a default route; check it anyway.
sleep 1
if [ -n "$("${compose[@]}" exec -T bob ip route show default 2>&1)" ]; then
    echo "FAIL: bob has a route to the internet"
    exit 1
fi

# alice waits for discovery, lists peers, waits for the search to end
# (at most about 10 s), sends to peer 0, waits for the reply and quits.
echo "Running alice..."
alice_script='(sleep 5; echo list; sleep 12; echo "send 0 hello-from-lab"; sleep 3; echo quit) | p2p-app'
"${compose[@]}" run --rm -T alice sh -c "$alice_script" </dev/null >"$results/alice.log" 2>&1 || true

echo "Checking netem..."
if netem_error=$("${compose[@]}" exec -T bob tc qdisc add dev eth0 root netem delay 20ms 2>&1); then
    netem="available"
    "${compose[@]}" exec -T bob tc qdisc del dev eth0 root >/dev/null 2>&1 || true
else
    netem="NOT available ($netem_error)"
fi
echo "netem: $netem" | tee "$results/netem.txt"

"${compose[@]}" logs --no-color --no-log-prefix bob >"$results/bob.log" 2>&1

echo "--- alice ---"
cat "$results/alice.log"
echo "--- bob ---"
cat "$results/bob.log"
echo "-------------"

status=0
if ! grep -q "(lan)" "$results/alice.log"; then
    echo "alice did not list a peer marked (lan)"
    status=1
fi
if ! grep -qF "hello-from-lab" "$results/bob.log"; then
    echo "bob did not receive hello-from-lab"
    status=1
fi

if [ "$status" -eq 0 ]; then
    echo "PASS"
else
    echo "FAIL"
fi
exit "$status"
