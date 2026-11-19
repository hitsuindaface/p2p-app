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

`cargo lint` and `cargo xtask` are aliases defined in [`.cargo/config.toml`](.cargo/config.toml).
The xtask code is in [`xtask/src/main.rs`](xtask/src/main.rs).

## Using the chat

Start two copies, then in one of them:

```
/list           show the peers that were found
/connect 0      connect to peer number 0 in the list
hello           anything that does not start with / is sent
/disconnect
/quit
```

## Adding a dependency

If the build needs a new system package, add it to the right section of
`.dependencies` (`[apt]`, `[brew]` or `[winget]`) in the same pull request.
Rust crates go in `Cargo.toml` as usual and need no entry there.

## Directory server

The directory server lives in `directory-server/` as its own crate, so it builds
without the app's dependencies. It listens on UDP 47802 by default.

```bash
cargo run -r -p p2p-directory-server            # local run
podman build -f directory-server/Dockerfile . # container image
```

A push to `main` that changes `directory-server/` builds a `linux/arm64` image and
pushes it to `ghcr.io/hitsuindaface/p2p-directory-server` with the tags `latest` and
the commit SHA.
