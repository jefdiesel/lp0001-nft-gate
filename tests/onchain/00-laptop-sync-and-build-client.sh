#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# LAPTOP. Builds gate-cli (development receipts only, no `prove` feature) through
# the build wrapper and copies the repo and the binary to the build host's own folder.
set -euo pipefail
REPO=$SRC_ROOT/lez-nft-gate
G=$BUILD_ROOT/lp0001/gate
cd "$REPO/client"
the build wrapper cargo +1.98.1 build --release
BIN=$BUILD_ROOT/target/lez-nft-gate/release/gate-cli
"$BIN" --help > /dev/null
shasum -a 256 "$BIN"
ssh build-host "mkdir -p $G/out $G/repo"
rsync -a --delete --exclude target --exclude node_modules --exclude .git "$REPO/" "build-host:$G/repo/"
scp "$BIN" "build-host:$G/out/gate-cli"
ssh build-host "$G/out/gate-cli --help | head -3"
