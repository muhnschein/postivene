# postivene -- developer targets.
#
# `make check` is what CI runs, minus what needs the Sailfish SDK or a
# phone. It is not entirely offline: `msrv` fetches a toolchain the first
# time it runs, and `deny` wants the advisory database.
#
# Qt5 packages (Debian/Ubuntu):
#   apt install qtbase5-dev qtdeclarative5-dev qtdeclarative5-dev-tools \
#               qml-module-qtquick2
#
# The last is the QtQuick runtime plugin, which the -dev packages omit.

.PHONY: check test lint fmt qml-lint packaging-lint lockfile-lint doc-lint \
        msrv deny integration harbour vendor-check fetch-server clean

CARGO ?= cargo
# The shim's tests drive a real Qt event loop, which needs a platform
# plugin.
export QT_QPA_PLATFORM = offscreen

## What CI runs, in the same order. Keep in step with ci.yml.
##
## `msrv` fetches a toolchain the first time, so this is not quite
## network-free; `deny` needs the advisory database and is CI's job.
check: fmt lint test doc-lint msrv qml-lint lockfile-lint packaging-lint harbour \
       vendor-check deny

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

## The spec parses, the desktop entry is valid, the shell scripts are
## clean, every docs/*.md a comment points at exists. A missing tool is a
## SKIP here and a failure in CI (PACKAGING_LINT_STRICT=1).
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
	cd rust && $(CARGO) +1.75.0 check --workspace --all-targets

## What Harbour would reject, read off the sources. `sfdk check -s harbour`
## on a built RPM is the authority (docs/HARBOUR.md); this is what can be
## answered without the SDK, and CI runs it on every pull request.
##
## Wants `make fetch-server` and a built binary for its last few checks;
## without them it says so rather than passing quietly.
harbour:
	./ci/harbour-check.sh
	./ci/harbour-check-selftest.sh

## third_party/qmetaobject is upstream plus one three-line patch, and this
## proves it. Needs the network: it fetches the crates.io tarball to compare
## against.
vendor-check:
	./ci/vendor-check.sh

## Licences and advisories, as CI's `deny` job runs them. Needs
## `cargo install cargo-deny`, and the advisory database (network).
##
## A missing tool is a skip; a finding is a failure. The two used to share
## one `||`, which printed SKIP over a real advisory and let `make check`
## exit 0 on it.
deny:
	@command -v cargo-deny >/dev/null 2>&1 || \
		{ echo "deny: SKIP (cargo-deny not installed)"; exit 0; }
	cd rust && $(CARGO) deny check

## Fetch the pinned upstream deltachat-rpc-server binaries (network).
fetch-server:
	./scripts/fetch-rpc-server.sh

## The tests that drive the real core, offline. Needs `make fetch-server`.
integration:
	cd rust && DELTACHAT_RPC_SERVER=vendor/deltachat-rpc-server/x86_64/deltachat-rpc-server \
		$(CARGO) test -p deltachat-jsonrpc --test real_server -- --nocapture

clean:
	cd rust && $(CARGO) clean
