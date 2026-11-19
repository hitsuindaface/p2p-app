# p2p-app

A peer-to-peer messenger for the local network, written in Rust.

## Getting started

Everything the project needs is listed in [`.dependencies`](.dependencies).
One command installs all of it.

**Linux, WSL and macOS**

```bash
make setup
make build
```

**Windows** (PowerShell, in the repo folder)

```powershell
.\make setup
.\make build
```

`setup` uses `apt` on Ubuntu/Debian and WSL, Xcode Command Line Tools on macOS,
and `winget` on Windows. It may ask for your password or show installer windows.
Reopen the terminal when it finishes.

To check what is installed without changing anything, run `make doctor`
(Windows: `.\make doctor`).

## Commands

The same targets work on every platform. On Windows, write `.\make` in place of `make`.

| Command      | What it does                                         |
|--------------|------------------------------------------------------|
| `make recap` | Start of the day: latest code, Rust update, tool check, build |
| `make run`   | Start the chat. Start it in two terminals to test.  |
| `make test`  | Run all tests                                        |
| `make fmt`   | Format all code                                      |
| `make lint`  | Run clippy and treat warnings as errors              |
| `make check` | Format check, lint and tests: the same checks as CI |
| `make help`  | List every command                                   |

Run `make check` before you open a pull request.

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
