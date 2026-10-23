#!/usr/bin/env bash
# Builds the phone's chat core page (apps/mobile/app/assets/chat-core/chat-core.html) on a release
# runner: binaryen pinned by checksum, the gx-chat-wasm toolchain with its wasm32 target, then
# `cargo xtask build-mobile-chat-wasm`. Metro bundles the page as an asset, so it ships inside the
# app and in every over-the-air update.
#
#   tooling/release-mobile/chat-core-page.sh
#
# CDXC:Release 2026-10-23 SEE-ALSO: .github/workflows/release-gpui-android.yml and
# release-mobile-ios-testflight.yml run this before the app build; the page's runtime is
# apps/mobile/app/src/chat/rust/wasm-core/ (CDXC:SessionChat 2026-10-23 DECISION there).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../release-gpui/common.sh
source "$SCRIPT_DIR/../release-gpui/common.sh"
REPO_ROOT="$(release_gpui_repo_root)"

BINARYEN_VERSION=133
case "$(uname -s)-$(uname -m)" in
Linux-x86_64)
	BINARYEN_ASSET="binaryen-version_${BINARYEN_VERSION}-x86_64-linux.tar.gz"
	BINARYEN_SHA256=2dc9c7813f5375db93d96ead4b78222fcc3e2677bbb832297af4797782a37489
	;;
Darwin-arm64)
	BINARYEN_ASSET="binaryen-version_${BINARYEN_VERSION}-arm64-macos.tar.gz"
	BINARYEN_SHA256=ad66da82ac13f163e424b1643f16c6dfcccc98b5966296b43e52d3cab04f84a8
	;;
*)
	echo "No pinned binaryen for $(uname -s)-$(uname -m)" >&2
	exit 1
	;;
esac

TOOLS="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/binaryen-$BINARYEN_VERSION"
if [[ ! -x "$TOOLS/bin/wasm-opt" ]]; then
	mkdir -p "$TOOLS"
	ARCHIVE="$TOOLS.tar.gz"
	"$SCRIPT_DIR/../release-gpui/retry.sh" 3 5 -- curl -fsSL -o "$ARCHIVE" \
		"https://github.com/WebAssembly/binaryen/releases/download/version_${BINARYEN_VERSION}/${BINARYEN_ASSET}"
	ACTUAL="$(release_gpui_sha256 "$ARCHIVE")"
	if [[ "$ACTUAL" != "$BINARYEN_SHA256" ]]; then
		echo "binaryen checksum mismatch: $ACTUAL != $BINARYEN_SHA256" >&2
		exit 1
	fi
	tar xzf "$ARCHIVE" -C "$TOOLS" --strip-components=1
fi
export PATH="$TOOLS/bin:$PATH"

cd "$REPO_ROOT"
TOOLCHAIN="$(sed -n 's/^channel = "\(.*\)"$/\1/p' packages/gx-chat-wasm/rust-toolchain.toml)"
[[ -n "$TOOLCHAIN" ]] || {
	echo "packages/gx-chat-wasm/rust-toolchain.toml names no channel" >&2
	exit 1
}
"$SCRIPT_DIR/../release-gpui/retry.sh" 3 5 -- rustup toolchain install "$TOOLCHAIN" --profile minimal \
	--target wasm32-unknown-unknown
cargo xtask build-mobile-chat-wasm
test -s apps/mobile/app/assets/chat-core/chat-core.html
