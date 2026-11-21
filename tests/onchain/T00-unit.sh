#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# T0 (LAPTOP): gate_core and guest-logic unit tests, vectors, JS. Pass when all green.
# Runs on the laptop with plain cargo and node; no build wrapper, no remote paths. All paths come
# from settings below.
set -euo pipefail
: "${LP0001_REPO:?set LP0001_REPO to this repository's root on the laptop}"
: "${LP0001_T00_LOG:?set LP0001_T00_LOG to a writable log file path}"
: "${LP0001_CARGO_TARGET_DIR:?set LP0001_CARGO_TARGET_DIR to the cargo target dir on the laptop}"
REPO=$LP0001_REPO
LOG=$LP0001_T00_LOG
cd "$REPO"
python3 -I tools/gen_vectors.py --check
CARGO_TARGET_DIR=$LP0001_CARGO_TARGET_DIR cargo +1.98.1 test --workspace --no-fail-fast > "$LOG" 2>&1 || { grep -E "FAILED|panicked|^error" "$LOG"; echo "T00 FAIL: cargo test"; exit 1; }
grep -E "test result" "$LOG"
CARGO_TARGET_DIR=$LP0001_CARGO_TARGET_DIR cargo +1.98.1 check -p gate_program --features unsafe-test-gate
export NVM_DIR="${LP0001_NVM_DIR:-$HOME/.nvm}"; . "$NVM_DIR/nvm.sh" >/dev/null
(cd js/gate-verify && node --test test/*.test.mjs)
(cd js/wallet-shim && node --test test/*.test.mjs)
(cd miniapp && node --test test/*.test.mjs)
echo "T00 PASS"
