#!/bin/sh
# Fetch upstream deltachat-rpc-server binaries for the architectures
# Postivene packages, and place them where rpm/postivene.spec expects them:
#
#     vendor/deltachat-rpc-server/<sailfish-arch>/deltachat-rpc-server
#
# Upstream (chatmail/core) builds these statically against musl libc
# ("to avoid problems with glibc version incompatibility", per their CI
# workflow), so a binary depends only on the kernel -- this is why a
# generic upstream build is expected to run on Sailfish unmodified.
# On-device confirmation is still Milestone 1's final step.
#
# Source: the exact same binaries upstream attaches to its GitHub release
# are also published inside its PyPI wheels (see
# .github/workflows/deltachat-rpc-server.yml upstream: both are the nix
# build's `result/bin/deltachat-rpc-server`). PyPI is used here because
# it's fetchable with plain HTTPS and the wheel is just a zip.
#
# Everything is pinned: bump VERSION *and* refresh all checksums together
# (fetch the new wheels, run `sha256sum`, update below), then update
# vendor/deltachat-rpc-server/SOURCE.md to match -- the MPL-2.0
# source-availability notice must always describe the binaries actually
# bundled (see docs/LICENSING.md).

set -eu

VERSION="2.53.0"

# sailfish-arch  upstream-arch  wheel-tag                                                                     wheel-sha256                                                       binary-sha256
TABLE="
aarch64 aarch64 py3-none-manylinux_2_17_aarch64.manylinux2014_aarch64.musllinux_1_1_aarch64 9b777638e132eaf860b724d0521e4dca8de2d3976a3587f482b5be0f8bc2efcf 2df89ca213948e4557a11eff3ffff05efd46c0314374fc791309bd1b7fe6b769
armv7hl armv7l py3-none-linux_armv7l.manylinux_2_17_armv7l.manylinux2014_armv7l.musllinux_1_1_armv7l eb056b60a48506f38f28187eea2763a5e251d9179b2f31c1624e447b7e41d7d7 d7c20192ab29b0bc80e15a464b436a0ffcc4b0e21c4f43f3fffc6c5268410645
x86_64 x86_64 py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64.musllinux_1_1_x86_64 5d87ab664667f4a8e0926aa0e6428efe88fb37fb30a17abe00a9d777675e0cd0 dcc37af7b7e95aae714c2366ff9f6d5c64170d0a7bb72cd7e3926a1a46e7e750
"

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
vendor_dir="$repo_root/vendor/deltachat-rpc-server"
workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

fetch_one() {
    sfos_arch="$1"; upstream_arch="$2"; wheel_tag="$3"; wheel_sha="$4"; bin_sha="$5"

    wheel="deltachat_rpc_server-$VERSION-$wheel_tag.whl"

    echo ">> $sfos_arch (upstream $upstream_arch)"
    # pip resolves PyPI's hash-addressed file URL; --no-deps and the exact
    # version pin keep it honest, and the sha256 check below keeps it
    # honest even if the index were tampered with.
    pip download "deltachat-rpc-server==$VERSION" --no-deps \
        --platform "musllinux_1_1_$upstream_arch" --only-binary=:all: \
        -d "$workdir" >/dev/null

    echo "$wheel_sha  $workdir/$wheel" | sha256sum -c - >/dev/null

    unzip -oq "$workdir/$wheel" -d "$workdir/$sfos_arch"
    bin="$workdir/$sfos_arch/deltachat_rpc_server/deltachat-rpc-server"
    echo "$bin_sha  $bin" | sha256sum -c - >/dev/null

    install -Dm 755 "$bin" "$vendor_dir/$sfos_arch/deltachat-rpc-server"
    echo "   -> vendor/deltachat-rpc-server/$sfos_arch/deltachat-rpc-server"
}

echo "$TABLE" | while read -r sfos_arch upstream_arch wheel_tag wheel_sha bin_sha; do
    [ -n "$sfos_arch" ] || continue
    fetch_one "$sfos_arch" "$upstream_arch" "$wheel_tag" "$wheel_sha" "$bin_sha"
done

echo "Done. Binaries are deltachat-rpc-server v$VERSION, statically linked (musl)."
echo "Remember: vendor/deltachat-rpc-server/SOURCE.md must describe this exact version."
