# SPDX-License-Identifier: MIT OR Apache-2.0
# Sourced by every on-chain test script that runs on the BUILD HOST. All machine-specific paths
# come from the settings below; none has a private default. Run each script on the machine its
# header names. See tests/onchain/README.md for the full list and examples.
#
# Rules (brief §6): own folder only; the job-a tree is read-only (its r0vm for simulation, its
# patched spel, its chain on 3310). Never r0vm on PATH. No real proof in the default (dev) mode.

# ---- required settings: an unset one stops here with a plain message ----
: "${LP0001_GATE_HOME:?set LP0001_GATE_HOME to this run own folder (out/, wallet/, run/, guest-target/)}"
: "${LP0001_REPO:?set LP0001_REPO to the repository root on this machine}"
: "${LP0001_SPEL:?set LP0001_SPEL to the spel CLI binary (deploy and program-id)}"
: "${LP0001_PACKAGER:?set LP0001_PACKAGER to the risc0-packager binary (guest builds)}"
: "${LP0001_SEQ_RPC:?set LP0001_SEQ_RPC to this chains JSON-RPC URL}"

G=$LP0001_GATE_HOME
R=$LP0001_REPO
SPEL=$LP0001_SPEL
PACK=$LP0001_PACKAGER
RPC=$LP0001_SEQ_RPC
export GATE_CLI_SEQUENCER=$RPC
export LEE_WALLET_HOME_DIR=$G/wallet
export GATE_CONFIG=$G/gate-config.json

# Optional settings with generic (non-private) defaults.
export PATH="${LP0001_EXTRA_PATH:-}:/usr/bin:/bin:/usr/sbin:/sbin:$HOME/.cargo/bin"
CLI=${LP0001_CLI:-$G/out/gate-cli}
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
elif [ "${LP0001_IN_PROCESS:-0}" = 1 ]; then
  # Development receipts with no r0vm: gate-cli built with `prove` runs the executor in this
  # process, and with RISC0_DEV_MODE=1 its local prover only makes development receipts.
  unset RISC0_SERVER_PATH
  export RISC0_DEV_MODE=1 RISC0_PROVER=local RISC0_EXECUTOR=local RAYON_NUM_THREADS=4
  case $CLI in *gate-cli-prove) ;; *) echo "STOP: LP0001_IN_PROCESS needs gate-cli-prove, not $CLI"; exit 1 ;; esac
  [ -x "$CLI" ] || { echo "STOP: $CLI missing (90-build-cli-prove.sh)"; exit 1; }
else
  : "${LP0001_R0VM:?set LP0001_R0VM to the r0vm binary used for local simulation}"
  export RISC0_DEV_MODE=1
  export RISC0_SERVER_PATH=$LP0001_R0VM
  [ -x "$RISC0_SERVER_PATH" ] || { echo "STOP: $RISC0_SERVER_PATH missing: no simulation possible"; exit 1; }
fi
mkdir -p "$G/run"
