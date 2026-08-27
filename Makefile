# postivene -- developer targets.
#
# `make check` is what CI runs, minus the parts that need a network fetch or
# a Sailfish SDK. Everything here works from a clean checkout with a host
# Rust toolchain and Qt5 dev packages; nothing needs a phone.
#
# Qt5 dev packages (Debian/Ubuntu):
#   apt install qtbase5-dev qtdeclarative5-dev qtdeclarative5-dev-tools \
#               qml-module-qtquick2
#
# The last one is the QtQuick runtime plugin. It is not pulled in by the
# -dev packages, and without it every QML the tests load fails with
# `module "QtQuick" is not installed`.

.PHONY: check test lint fmt qml-lint packaging-lint lockfile-lint doc-lint \
        msrv integration fetch-server clean

CARGO ?= cargo
# The shim's tests drive a real Qt event loop; without a display that has to
# be the offscreen platform plugin. Set here rather than in each test so a
# bare `cargo test` and `make test` behave the same way.
export QT_QPA_PLATFORM = offscreen

## Everything that runs anywhere, in the order CI runs it.
check: fmt lint test qml-lint lockfile-lint packaging-lint

## Unit, integration, and Qt event-loop tests.
test:
	cd rust && $(CARGO) test --workspace

## Clippy at the workspace lint level (pedantic + the bans in
## rust/clippy.toml), over tests and binaries too.
lint:
	cd rust && $(CARGO) clippy --workspace --all-targets -- -D warnings

fmt:
	cd rust && $(CARGO) fmt --all --check

## Parse every .qml file. The Qt-5.6 dialect rules qmllint cannot express
## are a Rust test instead (postivene-shim/tests/qml_syntax.rs).
qml-lint:
	./ci/qml-lint.sh

## The spec parses, the desktop entry is valid, the shell scripts are clean.
packaging-lint:
	./ci/packaging-lint.sh

## Cargo.lock must stay v3: Sailfish's cargo 1.75 cannot read v4.
lockfile-lint:
	./ci/check-lockfile.sh

## Broken intra-doc links are errors, not warnings nobody reads.
doc-lint:
	cd rust && RUSTDOCFLAGS="-D warnings" $(CARGO) doc --workspace --no-deps

## Compile against the toolchain floor Sailfish actually ships. The device
## build is the only one that matters in the end, and it does not get to
## pick a newer rustc.
msrv:
	rustup toolchain install 1.75.0 --profile minimal
	cd rust && RUSTFLAGS="-D warnings" $(CARGO) +1.75.0 check --workspace --all-targets

## Fetch the pinned upstream deltachat-rpc-server binaries (network).
fetch-server:
	./scripts/fetch-rpc-server.sh

## The tests that drive the *real* Delta Chat core, offline but for real.
## Needs `make fetch-server` first; skipped silently without it, which is
## why CI runs the fetch as its own step and fails there instead.
integration:
	cd rust && DELTACHAT_RPC_SERVER=vendor/deltachat-rpc-server/x86_64/deltachat-rpc-server \
		$(CARGO) test -p deltachat-jsonrpc --test real_server -- --nocapture

clean:
	cd rust && $(CARGO) clean
