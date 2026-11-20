#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# BUILD HOST ONLY, through the lock (the laptop has no `metal` tool; risc0-sys 1.5.1 compiles
# Metal kernels with `prove`, F §11):
#   ssh build-host '$BUILD_ROOT/dev/fix-names/mini-lock.sh $BUILD_ROOT/lp0001/gate/repo/tests/onchain/90-build-cli-prove.sh > $BUILD_ROOT/lp0001/gate/run/build-cli-prove.log 2>&1'
set -euo pipefail
G=${LP0001_GATE_HOME:-$BUILD_ROOT/lp0001/gate}
export PATH=$HOME/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin
export CARGO_TARGET_DIR=$G/cli-prove-target CARGO_BUILD_JOBS=4 TMPDIR=$G/tmp
mkdir -p $G/tmp
cargo +1.98.1 build --release --features prove --manifest-path $G/repo/client/Cargo.toml
cp $G/cli-prove-target/release/gate-cli $G/out/gate-cli-prove
shasum -a 256 $G/out/gate-cli-prove
