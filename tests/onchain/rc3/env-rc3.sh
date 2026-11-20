# SPDX-License-Identifier: MIT OR Apache-2.0
# Point the on-chain scripts at OUR public LEZ v0.4.0-rc3 chain (https://lez.nameswallet.app/,
# loopback 127.0.0.1:3290 on the Mini) instead of the throwaway chain on 3310. Exports only; every
# script still sources ../env.sh, which reads these. Use:  rc3/with-rc3.sh <script> [args]
#
# That chain verifies REAL receipts (no RISC0_DEV_MODE on its sequencer): every private
# transaction is a real proof, about 8-12 min and ~9.4 GB on the Mini. So:
#  - only gate-cli built with `prove` (90-build-cli-prove.sh), in-process prover, 4 threads;
#  - every private step runs under mini-lock.sh and the rfp-022 guard.sh, one at a time;
#  - TEST ONLY programs (unsafe gate, T4 hostile) are never built or deployed here;
#  - the full T01-T15 suite is NOT run here (40+ proofs): rc3/02-setup-rc3.sh (3 proofs) and
#    rc3/E2E-rc3.sh (1 proof, T08's on-chain use) are.
export LP0001_GATE_HOME=$BUILD_ROOT/lp0001/rc3-run
export LP0001_RPC=http://127.0.0.1:3290
# Logos's reference NFT program on rc3 (in our program registry; image 4fbb2872..., the same
# image as job-a's nft.bin built from lez-programs dea23c2).
export LP0001_NFT=CjrUpegdA1RjqDN43T8P19kAkwnD1quAkNkER8k3dw9G
export LP0001_CLI=$LP0001_GATE_HOME/out/gate-cli-prove
export LP0001_REAL_PROVING=1
export LP0001_NO_TEST_PROGRAMS=1
export LP0001_SUBMISSION_LABEL=lp0001-gate/test-submission/public-rc3
export LP0001_DEPLOY_NOTE="TEST deployment on our public rc3 chain (lez.nameswallet.app); submission_id is a test value"
export LP0001_MINILOCK=$BUILD_ROOT/dev/fix-names/mini-lock.sh
export LP0001_GUARD=$BUILD_ROOT/rfp-022/lez-bridge/scripts/guard.sh
# The chain's faucet wallet (read for one funding transfer; its process and config untouched).
export LP0001_FAUCET_ENV=$BUILD_ROOT/rfp-022/public-chain/faucet/env
