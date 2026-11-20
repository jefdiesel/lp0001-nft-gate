# SPDX-License-Identifier: MIT OR Apache-2.0
# Sourced by every on-chain test script. Build host (ssh build-host) only.
# Rules: LP0001-GLM-BRIEF-2026-11-18.md §6. Own folder only; job-a is read-only (its r0vm for
# simulation, its patched spel, its chain on 3310). Never r0vm on PATH. No real proof here.
# G is this run's own folder (repo copy, out/, wallet/, run/, guest-target/). LP0001_GATE_HOME
# points a second run at a folder of its own; the default is the first run's folder.
G=${LP0001_GATE_HOME:-$BUILD_ROOT/lp0001/gate}
J=$BUILD_ROOT/lp0001/job-a
R=$G/repo
export LEE_WALLET_HOME_DIR=$G/wallet
export GATE_CONFIG=$G/gate-config.json
# LP0001_RPC, LP0001_NFT and LP0001_CLI point a run at another chain (tests/onchain/rc3/env-rc3.sh:
# our public rc3 chain); the defaults are the throwaway chain's.
export GATE_CLI_SEQUENCER=${LP0001_RPC:-http://127.0.0.1:3310}
export PATH=$HOME/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin
CLI=${LP0001_CLI:-$G/out/gate-cli}
SPEL=$J/out/spel
RPC=$GATE_CLI_SEQUENCER
# The reference NFT program on the throwaway chain (native build, F §0, §6).
NFT=${LP0001_NFT:-ErycMYqXUUtducaYmpHZjRgdbLXchrMiV4NZRnmRDNmu}
export GATE_NFT_PROGRAM=$NFT
if command -v r0vm >/dev/null 2>&1; then echo "STOP: r0vm is on PATH (brief §6)"; exit 1; fi
if [ "${LP0001_REAL_PROVING:-0}" = 1 ]; then
  # A chain that verifies real receipts (the public rc3 chain): no development receipts, no r0vm
  # server, only the in-process CPU prover on 4 threads, and only gate-cli built with `prove`
  # (a CLI without it would fall back to an external r0vm on all cores: the Mini's panic path).
  unset RISC0_DEV_MODE RISC0_SERVER_PATH
  export RISC0_PROVER=local RAYON_NUM_THREADS=4
  case $CLI in *gate-cli-prove) ;; *) echo "STOP: real proving needs gate-cli-prove, not $CLI"; exit 1 ;; esac
  [ -x "$CLI" ] || { echo "STOP: $CLI missing (90-build-cli-prove.sh)"; exit 1; }
else
  export RISC0_DEV_MODE=1
  export RISC0_SERVER_PATH=$J/tools/r0vm
  [ -x "$RISC0_SERVER_PATH" ] || { echo "STOP: $RISC0_SERVER_PATH missing: no simulation possible"; exit 1; }
fi
mkdir -p "$G/run"
