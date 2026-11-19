# SPDX-License-Identifier: MIT OR Apache-2.0
# Sourced by every on-chain test script. Build host (ssh build-host) only.
# Rules: LP0001-GLM-BRIEF-2026-11-18.md §6. Own folder only; job-a is read-only (its r0vm for
# simulation, its patched spel, its chain on 3310). Never r0vm on PATH. No real proof here.
G=$BUILD_ROOT/lp0001/gate
J=$BUILD_ROOT/lp0001/job-a
R=$G/repo
export LEE_WALLET_HOME_DIR=$G/wallet
export GATE_CONFIG=$G/gate-config.json
export GATE_CLI_SEQUENCER=http://127.0.0.1:3310
export RISC0_DEV_MODE=1
export RISC0_SERVER_PATH=$J/tools/r0vm
export PATH=$HOME/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin
CLI=$G/out/gate-cli
SPEL=$J/out/spel
RPC=http://127.0.0.1:3310
# The reference NFT program on the throwaway chain (native build, F §0, §6).
NFT=ErycMYqXUUtducaYmpHZjRgdbLXchrMiV4NZRnmRDNmu
export GATE_NFT_PROGRAM=$NFT
if command -v r0vm >/dev/null 2>&1; then echo "STOP: r0vm is on PATH (brief §6)"; exit 1; fi
[ -x "$RISC0_SERVER_PATH" ] || { echo "STOP: $RISC0_SERVER_PATH missing: no simulation possible"; exit 1; }
mkdir -p "$G/run"
