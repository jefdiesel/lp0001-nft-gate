#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# DRAFT (row 36): prepare a fresh LEZ v0.4.0-rc3 standalone chain, fund a wallet from published
# local-test genesis accounts, build/deploy the programs, and run the on-chain tests that work
# with development receipts. Called by .github/workflows/e2e.yml; no local paths or hosts here.
#
# Required env (set by the workflow): SEQ_BIN LOCAL_JSON CLI SPEL NFT_SRC RPC WORKSPACE E2E_DIR
# Optional published local-test key (not a secret): PAYER_PRIVATE_KEY
set -uo pipefail
for v in SEQ_BIN LOCAL_JSON CLI SPEL NFT_SRC RPC WORKSPACE E2E_DIR; do
  [ -n "${!v:-}" ] || { echo "ci-run: $v is required" >&2; exit 2; }
done
die() { echo "ci-run: $*" >&2; exit 1; }
PAYER_PRIVATE_KEY=${PAYER_PRIVATE_KEY:-$("$(dirname "${BASH_SOURCE[0]}")/../../tools/lez-test-key.sh" PRIVATE_KEY_PUB_ACC_A)}
[ -n "$PAYER_PRIVATE_KEY" ] || die "failed to fetch PAYER_PRIVATE_KEY"
SUBMISSION=lp0001-gate/draft-ci-submission

G=$E2E_DIR/gate
W=$G/wallet
S=$E2E_DIR/scripts
B=$E2E_DIR/bin
mkdir -p "$G/run" "$W" "$S" "$B"
export LEE_WALLET_HOME_DIR=$W
export GATE_CONFIG=$G/gate-config.json
export GATE_CLI_SEQUENCER=$RPC
export RISC0_DEV_MODE=1
unset RISC0_SERVER_PATH RISC0_PROVER

x() { "$@" > "$E2E_DIR/setup.log" 2>&1 || { tail -40 "$E2E_DIR/setup.log" >&2; die "setup step failed: $*"; }; }
acct() { "$CLI" wallet account list 2>/dev/null | grep -F "[$1]" | grep -oE '(Public|Private)/[1-9A-HJ-NP-Za-km-z]{32,44}' | sed 's|^Public/||; s|^Private/||' | head -1; }
hex_of() { python3 -I - "$1" <<'PY'
import sys
A='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
n=0
for c in sys.argv[1]: n=n*58+A.index(c)
print(n.to_bytes(32,'big').hex())
PY
}
b58pk() { "$CLI" wallet account get --keys --account-id "Public/$1" 2>"$E2E_DIR/setup.log" | awk '/^pk /{print $2}'; }
# cargo risczero puts the deployable .bin next to the ELF; find it under the crate's target dir.
guest_bin() { find "$1/target/riscv32im-risc0-zkvm-elf/release" -maxdepth 1 -name '*.bin' 2>/dev/null | head -1; }
build_guest() { # dir  [bin name]
  local out
  x env CARGO_TARGET_DIR="$1/target" cargo risczero build --release --manifest-path "$1/Cargo.toml"
  out=$(guest_bin "$1")
  [ -n "$out" ] || die "no .bin produced by $1 (cargo risczero build)"
  cp "$out" "$B/${2:-$(basename "$out")}"
}
deploy() {
  local out
  "$SPEL" deploy "$1" --fee-payer "$(acct pay)" ${2:-} > "$E2E_DIR/setup.log" 2>&1 \
    || { tail -40 "$E2E_DIR/setup.log" >&2; return 1; }
  out=$(tail -1 "$E2E_DIR/setup.log" | tr -d '[:space:]')
  [[ "$out" =~ ^[1-9A-HJ-NP-Za-km-z]{32,44}$ ]] || { tail -40 "$E2E_DIR/setup.log" >&2; return 1; }
  printf '%s\n' "$out"
}

# ---- a fresh chain home and a running standalone sequencer ----
SEQ_HOME=$E2E_DIR/seq
RPC_ADDR=${RPC#http://}
RPC_ADDR=${RPC_ADDR#https://}
RPC_ADDR=${RPC_ADDR%%/*}
"$SEQ_BIN" setup "$SEQ_HOME" --config "$LOCAL_JSON" \
  --funding-public-key 0101010101010101010101010101010101010101010101010101010101010100 \
  --create-channel > "$E2E_DIR/sequencer-setup.log" 2>&1 || { cat "$E2E_DIR/sequencer-setup.log" >&2; die "sequencer setup"; }
"$SEQ_BIN" start "$SEQ_HOME" --rpc-listen-address "$RPC_ADDR" > "$E2E_DIR/sequencer.log" 2>&1 &
SEQ_PID=$!
trap 'kill $SEQ_PID 2>/dev/null || true' EXIT
SEQ_READY=0
for _ in $(seq 1 120); do
  if curl -s -X POST "$RPC" -H content-type:application/json \
    -d '{"jsonrpc":"2.0","id":1,"method":"getLastBlockId","params":[]}' | grep -q '"result"'; then
    SEQ_READY=1
    break
  fi
  if ! kill -0 "$SEQ_PID" 2>/dev/null; then
    cat "$E2E_DIR/sequencer.log" >&2
    die "sequencer exited during startup"
  fi
  sleep 1
done
if [ "$SEQ_READY" != 1 ]; then
  cat "$E2E_DIR/sequencer.log" >&2
  die "sequencer did not become ready on $RPC"
fi

# ---- wallet: labels, persisted local RPC, funding from published genesis account ----
[ -e "$W/storage.json" ] || x "$CLI" wallet account new public --label pay
x "$CLI" wallet change-network "$RPC"
for l in pay defn mstr actA actB atk; do [ -n "$(acct "$l")" ] || x "$CLI" wallet account new public --label "$l"; done
for l in h1 h2 h3 h4 h5 h6 gas; do [ -n "$(acct "$l")" ] || x "$CLI" wallet account new private --label "$l"; done
# Never pass PAYER_PRIVATE_KEY through x() or print it on failure.
GEN_PUB=$("$CLI" wallet account import public --private-key "$PAYER_PRIVATE_KEY" 2>/dev/null \
  | awk '/^Imported public account Public\//{sub(/^Public\//, "", $4); print $4}')
unset PAYER_PRIVATE_KEY
[ -n "$GEN_PUB" ] || die "failed to import genesis public account"
PAY_ID=$(acct pay)
[ -n "$PAY_ID" ] || die "no account id for pay"
PAY_PK=$(b58pk "$PAY_ID")
[ ${#PAY_PK} = 64 ] || { tail -40 "$E2E_DIR/setup.log" >&2; die "no public key for pay"; }
x "$CLI" wallet auth-transfer send --from "Public/$GEN_PUB" --to-pk "$PAY_PK" --amount 2000000000
# Bootstrap private `gas` by shielding from `pay` (wallet auto_fee_payer pays from what it receives).
x "$CLI" wallet auth-transfer send --from pay --to "Private/$(acct gas)" --amount 200000000
for l in h1 h2 h3 h4 h5 h6; do
  K=$("$CLI" wallet account show-keys --account-id "Private/$(acct "$l")" 2>"$E2E_DIR/setup.log")
  NPK=$(echo "$K" | sed -n 1p); VPK=$(echo "$K" | sed -n 2p)
  [ ${#NPK} = 64 ] && [ ${#VPK} = 2368 ] || { tail -40 "$E2E_DIR/setup.log" >&2; die "no keys for $l"; }
  x "$CLI" wallet --fee-payer "$(acct gas)" auth-transfer send --from pay --to-npk "$NPK" --to-vpk "$VPK" --amount 200000000
  POS=$("$CLI" wallet pending list 2>"$E2E_DIR/setup.log" | grep -E "^Message [0-9]+: to $(acct "$l") " | tail -1 | grep -oE '^Message [0-9]+' | grep -oE '[0-9]+')
  [ -n "$POS" ] || { tail -40 "$E2E_DIR/setup.log" >&2; die "no pending credit for $l"; }
  x "$CLI" wallet pending receive --position "$POS"
done
for l in actA actB atk; do x "$CLI" wallet auth-transfer send --from pay --to "Public/$(acct "$l")" --amount 50000000; done

# ---- programs. The NFT program must exist first: its deployed id is a build-time constant of
# the gate and consumer guests, so they are built after it with that id. ----
build_guest "$NFT_SRC" nft.bin
NFT=$(deploy "$B/nft.bin") || die "NFT deploy"
export GATE_NFT_PROGRAM=$NFT
export LP0001_NFT_PROGRAM=$(hex_of "$NFT")
build_guest "$WORKSPACE/guests/gate" lp0001-gate.bin
GATE=$(deploy "$B/lp0001-gate.bin" --immutable) || die "gate deploy"
GATE_HEADER=$("$CLI" header --program "$GATE" 2>"$E2E_DIR/setup.log") || { tail -40 "$E2E_DIR/setup.log" >&2; die "gate header"; }
echo "$GATE_HEADER" | grep -q "^immutable true$" || die "gate header is not immutable"
GATE_IMAGE_ID=$(echo "$GATE_HEADER" | awk '/^image_id /{print $2}')
[ ${#GATE_IMAGE_ID} = 64 ] || die "no gate image_id"
build_guest "$WORKSPACE/guests/gate-unsafe-test" lp0001-gate-unsafe-test.bin
TGATE=$(deploy "$B/lp0001-gate-unsafe-test.bin") || die "unsafe test gate deploy"
DEFN=$(acct defn)
[ -n "$DEFN" ] || die "no account id for defn"
ZONE=$("$CLI" channel-id 2>"$E2E_DIR/setup.log" | tail -1 | tr -d '[:space:]')
[ ${#ZONE} = 64 ] || { tail -40 "$E2E_DIR/setup.log" >&2; die "no channel-id"; }
export LP0001_GATE=$(hex_of "$GATE") LP0001_NFT_PROGRAM LP0001_COLLECTION=$(hex_of "$DEFN") \
       LP0001_SUBMISSION_ID=$(python3 -I -c "import hashlib,sys;print(hashlib.sha256(sys.argv[1].encode()).hexdigest())" "$SUBMISSION") \
       LP0001_ZONE_ID=$ZONE
build_guest "$WORKSPACE/guests/consumer" lp0001-consumer.bin
CONSUMER=$(deploy "$B/lp0001-consumer.bin") || die "consumer deploy"
export LP0001_T4_REAL_NFT="$LP0001_NFT_PROGRAM" \
       LP0001_T4_ATTACKER=$(hex_of "$(acct atk)") LP0001_T4_COLLECTION="$LP0001_COLLECTION"
build_guest "$WORKSPACE/tests-only/hostile-guest" lp0001-t4-hostile.bin
HOSTILE=$(deploy "$B/lp0001-t4-hostile.bin") || die "hostile deploy"

python3 -I - "$GATE_CONFIG" "$GATE" "$GATE_IMAGE_ID" "$TGATE" "$CONSUMER" "$HOSTILE" "$ZONE" "$DEFN" "$RPC" "$NFT" "$LP0001_SUBMISSION_ID" <<'PY'
import json, sys
cfg, gate, gate_image_id, tgate, consumer, hostile, zone, defn, rpc, nft, sub_id = sys.argv[1:12]
json.dump({
  "sequencer": rpc, "gate": gate, "gate_image_id": gate_image_id, "unsafe_test_gate": tgate,
  "consumer": consumer, "t4_hostile": hostile, "zone_id": zone, "collection": defn,
  "nft_program": nft, "submission_id": sub_id, "max_fee": "6000000",
}, open(cfg, "w"), indent=1)
PY

# ---- a shadow tests dir whose env.sh has no build-host settings ----
cp "$WORKSPACE"/tests/onchain/T*.sh "$WORKSPACE"/tests/onchain/lib.sh "$S"/
cat > "$S/env.sh" <<EOS
# CI env: a fresh local chain, development receipts, no build host.
G=$G
R=$WORKSPACE
CLI=$CLI
SPEL=$SPEL
RPC=$RPC
export GATE_CLI_SEQUENCER=\$RPC
export LEE_WALLET_HOME_DIR=$W
export GATE_CONFIG=$GATE_CONFIG
export GATE_NFT_PROGRAM=$NFT
export RISC0_DEV_MODE=1
unset RISC0_SERVER_PATH RISC0_PROVER LP0001_REAL_PROVING
mkdir -p "\$G/run"
EOS

# ---- collection: definition, holdings, one copy each for h1/h3/h4/h6 ----
x "$CLI" nft new-definition --definition defn --master mstr --name "LP0001 CI" --supply 8 --payer pay
for l in h1 h2 h3 h4 h5 h6 atk; do x "$CLI" nft init-holding --definition "$DEFN" --from "$l" --payer "$l"; done
for l in h1 h3 h4 h6; do x "$CLI" nft print --from mstr --to "$l" --payer "$l"; done
x "$CLI" nft pending

# ---- tests that dev mode can exercise, in dependency order ----
# Left out: T00 (laptop unit tests, already in ci.yml), 90/91 and BENCH-* (real proving),
# and rc3/* (our deployed chain, real proofs). See the reply and row 36.
TESTS="T03-unauthorized-root T06-window T01-holder-proves T02-non-member T05-gas-payer
       T07-reprove T08-consumer T09-transfer-then-prove T10-agreement T11-linkability
       T12-immutable T13-collision T14-timestamps T15-nullifier-replay T04-hostile"
failed=0
for t in $TESTS; do
  echo "== $t =="
  "$S/$t.sh" || { echo "($t did not pass; continuing)"; failed=1; }
done
cat "$G/run/RESULTS.txt" 2>/dev/null || true
exit $failed
