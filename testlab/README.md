# Test lab

The test lab runs p2p-app nodes in Docker containers. Each container is a fresh
install: it starts with no saved data and a new identity. The containers share
one private network. Before the app starts, `offline.sh` removes the
container's default route, so the nodes can reach each other but not the
internet. They can only find each other on the "LAN" (mDNS) and never touch the
public DHT or our relay.

We do not use a Docker network with `internal: true` for this: Docker then also
drops multicast between the containers, and mDNS stops working.

Later tests will use `tc netem` inside the containers to make the network slow
or lossy on purpose, and run the criterion 1 and 2 measurements many times in a row.

## Files

| File | What it is |
| --- | --- |
| `Dockerfile` | Builds the `p2p-app` binary and puts it in a small Debian image with `tc` |
| `compose.yaml` | Two nodes, `alice` and `bob`, on one network |
| `offline.sh` | Entry point of the image: cuts the container off from the internet, then starts the command |
| `smoke.sh` | The smoke test: do two nodes find each other and chat? |
| `results/` | Logs from the last run. Not in git. |

## Run the smoke test

You need Docker with the Compose plugin. On Windows and WSL, Docker Desktop with
WSL integration turned on for your distribution is enough. From the repo root:

```bash
bash testlab/smoke.sh
```

The first run takes several minutes, because it compiles the app in the
container. Later runs reuse the cached build.

The script:

1. builds the image,
2. starts `bob` and checks that it has no route to the internet,
3. starts `alice`, which waits 5 s, runs `list`, waits 12 s, runs
   `send 0 hello-from-lab` and quits,
4. checks whether `tc qdisc add dev eth0 root netem delay 20ms` works in a container,
5. saves the logs of both nodes in `testlab/results/` and stops everything with
   `docker compose down`.

## What the result means

**PASS** means alice listed a peer marked `(lan)` and bob printed the message
`hello-from-lab`. So two fresh nodes found each other with mDNS and delivered a
message. The script then exits with 0.

**FAIL** means one of those two things did not happen. The script prints which
one and exits with 1. Read `testlab/results/alice.log` and `bob.log`.

The script also prints `netem: available` or `netem: NOT available` (with the
error). This does not change PASS or FAIL. It tells us whether this machine can
run the network tests that slow down the link.

The same script runs every night on GitHub Actions
([`.github/workflows/lab.yml`](../.github/workflows/lab.yml)). The logs are
saved as a run artifact named `testlab-results`.
