# postivene -- developer targets.
#
# `make check` is what CI runs, minus what needs a network fetch or the
# Sailfish SDK. Nothing here needs a phone.
#
# Qt5 packages (Debian/Ubuntu):
#   apt install qtbase5-dev qtdeclarative5-dev qtdeclarative5-dev-tools \
#               qml-module-qtquick2
#
# The last is the QtQuick runtime plugin, which the -dev packages omit.

.PHONY: check test lint fmt qml-lint packaging-lint lockfile-lint doc-lint \
        msrv integration fetch-server clean

CARGO ?= cargo
# The shim's tests drive a real Qt event loop, which needs a platform
# plugin.
export QT_QPA_PLATFORM = offscreen

## What CI runs, in the same order. Keep in step with ci.yml.
check: fmt lint test doc-lint msrv qml-lint lockfile-lint packaging-lint

## Unit, integration, and Qt event-loop tests.
test:
	cd rust && $(CARGO) test --workspace

## Clippy at the workspace lint level, over tests and binaries too.
lint:
	cd rust && $(CARGO) clippy --workspace --all-targets -- -D warnings

fmt:
	cd rust && $(CARGO) fmt --all --check

## Parse every .qml file; the Qt 5.6 dialect rules are a Rust test.
qml-lint:
	./ci/qml-lint.sh

## The spec parses, the desktop entry is valid, the shell scripts are clean.
packaging-lint:
	./ci/packaging-lint.sh

## Cargo.lock must stay v3: Sailfish's cargo 1.75 cannot read v4.
lockfile-lint:
	./ci/check-lockfile.sh

## Broken intra-doc links are errors.
doc-lint:
	cd rust && RUSTDOCFLAGS="-D warnings" $(CARGO) doc --workspace --no-deps

## Compile against the toolchain floor Sailfish ships. Part of `check`:
## clippy on a modern toolchain does not reliably catch newer std methods or
## syntax, so only a real 1.75 build proves the device still builds.
msrv:
	rustup toolchain install 1.75.0 --profile minimal
	cd rust && RUSTFLAGS="-D warnings" $(CARGO) +1.75.0 check --workspace --all-targets

## Fetch the pinned upstream deltachat-rpc-server binaries (network).
fetch-server:
	./scripts/fetch-rpc-server.sh

## The tests that drive the real core, offline. Needs `make fetch-server`.
integration:
	cd rust && DELTACHAT_RPC_SERVER=vendor/deltachat-rpc-server/x86_64/deltachat-rpc-server \
		$(CARGO) test -p deltachat-jsonrpc --test real_server -- --nocapture

clean:
	cd rust && $(CARGO) clean
