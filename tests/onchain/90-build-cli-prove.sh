#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# BUILD HOST ONLY. Builds gate-cli with the `prove` feature (the laptop has no `metal` tool;
# risc0-sys 1.5.1 compiles Metal kernels with it). No build wrapper: the caller makes sure no other
# heavy build or proof is running at the same time. All paths come from env.sh.
set -euo pipefail
. "$(dirname "$0")/env.sh"
export CARGO_TARGET_DIR=$G/cli-prove-target CARGO_BUILD_JOBS=4 TMPDIR=$G/tmp
mkdir -p "$G/tmp"
cargo +1.98.1 build --release --features prove --manifest-path "$R/client/Cargo.toml"
cp "$G/cli-prove-target/release/gate-cli" "$G/out/gate-cli-prove"
shasum -a 256 "$G/out/gate-cli-prove"
