#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# BUILD HOST, through the lock:
#   ssh build-host '$BUILD_ROOT/dev/fix-names/mini-lock.sh $BUILD_ROOT/lp0001/gate/repo/tests/onchain/01-build-gate-guests.sh > $BUILD_ROOT/lp0001/gate/run/build-gate.log 2>&1'
# Builds the gate (pinned to the reference NFT program on 3310) and the TEST ONLY unsafe gate,
# with the reference guest flags (repo .cargo/config.toml), and packages them.
set -euo pipefail
. "$(dirname "$0")/env.sh"
export PATH=$HOME/.cargo/bin:$HOME/.risc0/bin:$PATH CARGO_BUILD_JOBS=4 TMPDIR=$G/tmp
mkdir -p "$G/tmp" "$G/out"
PACK=$J/guest-target/debug/risc0-packager          # job-a's packager, used read-only
export LP0001_NFT_PROGRAM=$(python3 -I "$R/tools/b58.py" hex $NFT)
build() { # dir bin out
  ( cd "$R/$1" && CARGO_TARGET_DIR=$G/guest-target/$3 cargo +risc0 build --release --target riscv32im-risc0-zkvm-elf )
  "$PACK" "$G/guest-target/$3/riscv32im-risc0-zkvm-elf/release/$2" "$G/out/$3.bin"
  echo "$3: $(wc -c < "$G/out/$3.bin") bytes, sha256 $(shasum -a 256 "$G/out/$3.bin" | cut -d' ' -f1), program id $("$SPEL" program-id "$G/out/$3.bin")"
}
build guests/gate lp0001-gate gate
build guests/gate-unsafe-test lp0001-gate-unsafe-test gate-unsafe-test
