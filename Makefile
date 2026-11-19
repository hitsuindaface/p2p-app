# Build commands for Linux and macOS. On Windows use make.ps1 (.\make <target>).
# Everything that gets installed is listed in .dependencies.

DEPS := .dependencies
OS   := $(shell uname -s)

# rustup installs into ~/.cargo/bin. Add it to PATH so the targets below work
# right after `make setup`, before the terminal has been reopened.
export PATH := $(HOME)/.cargo/bin:$(PATH)

# Print the items of one [section] in .dependencies, without comments.
section = $(shell awk -v s='[$(1)]' '/^\[/ {in_s = ($$1 == s); next} in_s { sub(/\043.*/, ""); sub(/\|.*/, ""); if (NF) print $$1 }' $(DEPS))

TOOLCHAIN  := $(call section,rust-toolchain)
COMPONENTS := $(call section,rust-components)
APT        := $(call section,apt)
BREW       := $(call section,brew)

.DEFAULT_GOAL := help
.PHONY: help setup system-deps rust doctor build release run test fmt lint check clean

help: ## Show this list
	@echo "Usage: make <target>"
	@echo
	@awk -F':.*## ' '/^[a-z-]+:.*## / {printf "  %-12s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

setup: system-deps rust ## Install everything in .dependencies
	@echo
	@echo "Setup done. Reopen the terminal so 'cargo' is on your PATH, then run: make build"

system-deps:
ifeq ($(OS),Linux)
	@command -v apt-get >/dev/null || { echo "This Makefile installs packages with apt. Install the [apt] packages in $(DEPS) with your own package manager."; exit 1; }
	sudo apt-get update
	sudo apt-get install -y $(APT)
else ifeq ($(OS),Darwin)
	@xcode-select -p >/dev/null 2>&1 || { xcode-select --install; echo "Finish the Xcode Command Line Tools installer, then run make setup again."; exit 1; }
	$(if $(strip $(BREW)),brew install $(BREW),@echo "No Homebrew packages needed.")
else
	@echo "Unsupported system: $(OS). On Windows, run .\make setup in PowerShell."; exit 1
endif

rust:
	@command -v rustup >/dev/null || curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none
	rustup toolchain install $(TOOLCHAIN) --profile minimal $(addprefix --component ,$(COMPONENTS))
	rustup default $(TOOLCHAIN)

doctor: ## Check that everything is installed, without changing anything
	@ok=1; \
	for t in rustup cargo; do command -v $$t >/dev/null && echo "  ok       $$t" || { echo "  missing  $$t"; ok=0; }; done; \
	for c in $(COMPONENTS); do rustup component list --installed 2>/dev/null | grep -q "^$$c" && echo "  ok       $$c" || { echo "  missing  $$c"; ok=0; }; done; \
	if [ "$(OS)" = Linux ] && command -v dpkg >/dev/null; then \
	  for p in $(APT); do dpkg -s $$p >/dev/null 2>&1 && echo "  ok       $$p" || { echo "  missing  $$p"; ok=0; }; done; \
	fi; \
	[ $$ok = 1 ] && echo "Everything is installed." || { echo "Run: make setup"; exit 1; }

build: ## Build a debug version
	cargo build

release: ## Build an optimised version
	cargo build --release

run: ## Start the chat (start it in two terminals to talk to yourself)
	cargo run --release

test: ## Run all tests
	cargo test

fmt: ## Format all code
	cargo fmt --all

lint: ## Run clippy and treat warnings as errors
	cargo clippy --all-targets -- -D warnings

check: ## Run the same checks as CI: format, lint, test
	cargo fmt --all --check
	cargo clippy --all-targets -- -D warnings
	cargo test

clean: ## Delete build output
	cargo clean
