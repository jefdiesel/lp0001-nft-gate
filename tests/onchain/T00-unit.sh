#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# T0 (LAPTOP): gate_core and guest-logic unit tests, vectors, JS. Pass when all green.
set -euo pipefail
REPO=$SRC_ROOT/lez-nft-gate
cd "$REPO"
python3 -I tools/gen_vectors.py --check
the build wrapper cargo +1.98.1 test --workspace --no-fail-fast 2>&1 | grep -E "test result|FAILED|panicked" || true
the build wrapper cargo +1.98.1 check -p gate_program --features unsafe-test-gate
export NVM_DIR="$HOME/.nvm"; . "$NVM_DIR/nvm.sh" >/dev/null
(cd js/gate-verify && node --test test/*.test.mjs)
(cd js/wallet-shim && node --test test/*.test.mjs)
(cd miniapp && node --test test/*.test.mjs)
echo "T00 done: every 'test result' above must say 0 failed"
