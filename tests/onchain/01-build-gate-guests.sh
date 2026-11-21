#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# BUILD HOST. Builds the gate (pinned to the reference NFT program on the throwaway chain) and the
# TEST ONLY unsafe gate, with the reference guest flags (repo .cargo/config.toml), and packages
# them. No build wrapper: the caller runs one guest build at a time. All paths come from env.sh.
set -euo pipefail
. "$(dirname "$0")/env.sh"
export CARGO_BUILD_JOBS=4 TMPDIR=$G/tmp
command -v r0vm >/dev/null 2>&1 && { echo "STOP: r0vm on PATH"; exit 1; }
mkdir -p "$G/tmp" "$G/out"
export LP0001_NFT_PROGRAM=$(python3 -I "$R/tools/b58.py" hex $NFT)
build() { # dir bin out
  ( cd "$R/$1" && CARGO_TARGET_DIR=$G/guest-target/$3 cargo +risc0 build --release --target riscv32im-risc0-zkvm-elf )
  "$PACK" "$G/guest-target/$3/riscv32im-risc0-zkvm-elf/release/$2" "$G/out/$3.bin"
  echo "$3: $(wc -c < "$G/out/$3.bin") bytes, sha256 $(shasum -a 256 "$G/out/$3.bin" | cut -d' ' -f1), program id $("$SPEL" program-id "$G/out/$3.bin")"
}
build guests/gate lp0001-gate gate
# TEST ONLY programs are never built for a public chain (LP0001_NO_TEST_PROGRAMS=1, rc3/env-rc3.sh).
[ "${LP0001_NO_TEST_PROGRAMS:-0}" = 1 ] || build guests/gate-unsafe-test lp0001-gate-unsafe-test gate-unsafe-test
