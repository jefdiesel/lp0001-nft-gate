# SPDX-License-Identifier: MIT OR Apache-2.0
# Point the on-chain scripts at OUR public LEZ v0.4.0-rc3 chain instead of the throwaway chain.
# Exports only; every script still sources ../env.sh, which reads these. Use:
#   rc3/with-rc3.sh <script> [args]
# This is a settings file: every machine-specific value must be supplied by the caller.
#
# That chain verifies REAL receipts (no RISC0_DEV_MODE on its sequencer): every private
# transaction is a real proof, about 8-12 min and ~9.4 GB. So:
#  - only gate-cli built with `prove` (90-build-cli-prove.sh), in-process prover, 4 threads;
#  - the caller runs one heavy proof at a time (there is no lock wrapper now);
#  - TEST ONLY programs (unsafe gate, T4 hostile) are never built or deployed here;
#  - the full T01-T15 suite is NOT run here (40+ proofs): rc3/02-setup-rc3.sh (3 proofs) and
#    rc3/E2E-rc3.sh (1 proof, T08's on-chain use) are.

: "${LP0001_GATE_HOME:?set LP0001_GATE_HOME to this rc3 run's own folder}"
: "${LP0001_REPO:?set LP0001_REPO to the repository root on this machine}"
: "${LP0001_SPEL:?set LP0001_SPEL to the spel CLI binary}"
: "${LP0001_PACKAGER:?set LP0001_PACKAGER to the risc0-packager binary}"
: "${LP0001_GUARD:?set LP0001_GUARD to the memory guard script}"
: "${LP0001_FAUCET_ENV:?set LP0001_FAUCET_ENV to the faucet wallet's env file}"
: "${LP0001_SEQ_RPC:?set LP0001_SEQ_RPC to the rc3 JSON-RPC URL}"

export LP0001_GATE_HOME LP0001_REPO LP0001_SPEL LP0001_PACKAGER
export LP0001_GUARD LP0001_FAUCET_ENV LP0001_SEQ_RPC
export LP0001_CLI=$LP0001_GATE_HOME/out/gate-cli-prove
export LP0001_REAL_PROVING=1
export LP0001_NO_TEST_PROGRAMS=1
export LP0001_SUBMISSION_LABEL=${LP0001_SUBMISSION_LABEL:-lp0001-gate/test-submission/public-rc3}
export LP0001_DEPLOY_NOTE=${LP0001_DEPLOY_NOTE:-"TEST deployment on our public rc3 chain; submission_id is a test value"}
# Logos's reference NFT program on rc3 (in our program registry; image 4fbb2872..., the same
# image as job-a's nft.bin built from lez-programs dea23c2).
export LP0001_NFT=${LP0001_NFT:-CjrUpegdA1RjqDN43T8P19kAkwnD1quAkNkER8k3dw9G}
