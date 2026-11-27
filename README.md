# p2p-app

A peer-to-peer messenger for the local network, written in Rust.

## Getting started

### 1. Install Rust and a C linker (once per machine)

Cargo cannot install these itself, so do this by hand the first time.

**Ubuntu, Debian and WSL**

```bash
sudo apt install build-essential curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**macOS**

```bash
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**Windows** (PowerShell)

```powershell
winget install Rustlang.Rustup
```

When the Rust installer asks to install the Visual Studio C++ build tools, say yes.
If you already have Visual Studio with "Desktop development with C++", that is enough.
If `cargo` later says `link.exe` was not found, the C++ tools are missing. Install them with:

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Open a new terminal afterwards so `cargo` is on your PATH.

### 2. Install everything else

```bash
cargo xtask setup
```

This reads [`.dependencies`](.dependencies) and installs what your system needs:
system libraries with `apt` on Linux, Xcode Command Line Tools on macOS, and
Perl and WebView2 with `winget` on Windows. It skips anything already installed.

To check what is installed without changing anything:

```bash
cargo xtask doctor
```

### 3. Build and run

```bash
cargo build
cargo run -r
```

## Commands

Everyday work is plain Cargo. `cargo xtask` covers the chores Cargo cannot do on its own.
The commands are the same on Windows, Linux and macOS.

| Command             | What it does                                                    |
|---------------------|-----------------------------------------------------------------|
| `cargo run -r`      | Start the chat. Start it in two terminals to test.              |
| `cargo test`        | Run the tests                                                   |
| `cargo fmt`         | Format the code                                                 |
| `cargo lint`        | Run clippy and treat warnings as errors                         |
| `cargo xtask check` | Format check, lint and tests: the same checks as CI             |
| `cargo xtask recap` | Start of the day: latest code, Rust update check, tool check, build |
| `cargo xtask setup` | Install everything in `.dependencies`                           |
| `cargo xtask doctor`| Check that everything is installed                              |

Run `cargo xtask check` before you open a pull request.
CI runs the same command on Windows, Linux and macOS for every pull request
([`.github/workflows/ci.yml`](.github/workflows/ci.yml)).

`cargo lint` and `cargo xtask` are aliases defined in [`.cargo/config.toml`](.cargo/config.toml).
The xtask code is in [`xtask/src/main.rs`](xtask/src/main.rs).

## Using the chat

Start two copies, then in one of them:

```
list            find peers: on the LAN first, then on the internet
send 0 hello    send "hello" to peer number 0 in the list
quit            exit (Ctrl+C does the same)
```

## Testing

[`docs/testing.md`](docs/testing.md) explains the kinds of tests, when a pull
request is done, and which test proves each success criterion and threat.

```bash
cargo test                       # unit, property and integration tests
cargo test -- --ignored          # also the tests that use the real network
bash testlab/smoke.sh            # two nodes in Docker (see testlab/README.md)
```

## Adding a dependency

If the build needs a new system package, add it to the right section of
`.dependencies` (`[apt]`, `[brew]` or `[winget]`) in the same pull request.
Rust crates go in `Cargo.toml` as usual and need no entry there.

## Networking

Peers find each other in two ways, without a central directory:

- **LAN:** mDNS.
- **Internet:** every running app announces itself as a provider of the key
  `p2penis/v1/<hour>` in the public IPFS DHT, and `list` asks for the current
  and previous hour. A new user can take up to about a minute to show up; a
  closed app drops out within two hours, because DHT servers cannot be told to
  forget an announcement.

Connections are encrypted and authenticated with Noise. Users behind a home
router are reached through a small libp2p relay, after which DCUtR tries to
punch a direct connection; if that fails, messages stay on the relay, which
limits each circuit to about 128 KiB and 2 minutes.

```bash
cargo run -r                                   # uses the default relays
cargo run -r -- /ip4/203.0.113.5/tcp/47802     # other relay(s)
```

The default relays are `DEFAULT_RELAYS` in `src/networking/node.rs`.

CLI commands: `list`, `send <index> <text>`, `quit`.

## Relay server

`relay-server/` is its own crate. It keeps no state and makes a new key on
every start; clients learn its PeerId when they connect. It needs a public
TCP port and its public address as the first argument; it listens on that address's port
unless a second argument gives another:

```bash
cargo run -r -p p2p-relay-server -- /dns4/relay.example.com/tcp/47802
podman build -f relay-server/Dockerfile .
```

A push to `main` that changes `relay-server/` builds a `linux/arm64` image and
pushes it to `ghcr.io/hitsuindaface/p2p-relay-server` with the tags `latest`
and the commit SHA.
