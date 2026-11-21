#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# LAPTOP. Builds gate-cli (development receipts only, no `prove` feature) with plain cargo and
# copies the repo and the binary into the build host's own run folder. No ssh, no build wrapper:
# LP0001_GATE_HOME must be the build host's run folder mounted on this machine.
set -euo pipefail
: "${LP0001_GATE_HOME:?set LP0001_GATE_HOME to the build host's run folder (mounted here)}"
: "${LP0001_REPO:?set LP0001_REPO to this repository's root on the laptop}"
: "${LP0001_CARGO_TARGET_DIR:?set LP0001_CARGO_TARGET_DIR to the cargo target dir on the laptop}"
REPO=$LP0001_REPO
G=$LP0001_GATE_HOME
cd "$REPO/client"
CARGO_TARGET_DIR=$LP0001_CARGO_TARGET_DIR cargo +1.98.1 build --release
BIN=$LP0001_CARGO_TARGET_DIR/release/gate-cli
"$BIN" --help > /dev/null
shasum -a 256 "$BIN"
mkdir -p "$G/out" "$G/repo"
rsync -a --delete --exclude target --exclude node_modules --exclude .git "$REPO/" "$G/repo/"
cp "$BIN" "$G/out/gate-cli"
"$G/out/gate-cli" --help | head -3
