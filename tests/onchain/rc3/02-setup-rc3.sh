#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# BUILD HOST, our public rc3 chain only (run through rc3/with-rc3.sh). The smallest setup that
# 02-setup-wallet.sh's flow allows on a chain that verifies real receipts: one private holder
# (h1) instead of seven, so three real proofs instead of ~20.
# Accounts: pay (public, funds and deploys), defn + mstr (public, the collection), actA, actB
# (public acting accounts), h1 (private, owns one copy).
#  1. pay is funded once from the chain's faucet wallet (FAUCET_OK=1; logged to
#     $G/run/FAUCET-TRANSFERS.log with the faucet's balance before and after).
#  2. actA, actB funded from pay (public); the collection created (public).
#  3. h1 funded by SHIELDING from pay into h1 (a private account of this same wallet, so h1
#     pays the fee out of what it receives; LEZ wallet auto_fee_payer, lib.rs:613-616). The
#     throwaway chain used a sealed send (--to-npk/--to-vpk) paid by a bootstrapped private
#     `gas` account instead; this chain has none. The shield shows pay -> (a commitment) in public;
#     it is test setup, not the gate's privacy path (spec §4.3).
#  4. init-holding and print to h1 (h1 pays). Steps 3-4 are the three real proofs.
# SETUP_PUBLIC_ONLY=1 stops after step 2 (to deploy before the proofs).
set -uo pipefail
D=$(cd "$(dirname "$0")/.." && pwd)
. "$D/env.sh"; . "$D/lib.sh" SETUP; . "$D/rc3/prove-lib.sh"
[ "${LP0001_REAL_PROVING:-0}" = 1 ] || { echo "STOP: run through rc3/with-rc3.sh"; exit 1; }
W="$CLI wallet"
stop() { VERDICT=FAIL; note "$1 failed (exit $(cat "$O/$1.rc")): $(grep -v '^\$ ' "$O/$1.out" | grep -m1 -iE 'error|fail|panic|revert|STOP' | cut -c1-300); $2"; finish_test; exit 1; }
[ -e "$G/wallet/storage.json" ] || run new $W account new public --label pay
run net $W change-network "$RPC"
for l in defn mstr actA actB; do [ -n "$(acct $l)" ] || run "new-$l" $W account new public --label $l; done
[ -n "$(acct h1)" ] || run new-h1 $W account new private --label h1
PAY=$(acct pay); H1=$(acct h1)
# 1. faucet -> pay (public, by public key: pay is a never-used account of another wallet)
run bal-pay0 $W account get --account-id "Public/$PAY"
PAYBAL=$(grep -oE "^Balance [0-9]+" "$O/bal-pay0.out" | awk '{print $2}')
if [ "${PAYBAL:-0}" -lt 50000000 ]; then
  [ "${FAUCET_OK:-0}" = 1 ] || { note "pay holds ${PAYBAL:-0}: set FAUCET_OK=1 to fund it once from the faucet wallet"; finish_test; exit 1; }
  PAY_PK=$($W account get --keys --account-id "Public/$PAY" 2>/dev/null | awk '/^pk /{print $2}')
  [ ${#PAY_PK} = 64 ] || { VERDICT=FAIL; note "no public key for pay (got '$PAY_PK')"; finish_test; exit 1; }
  FCLI=$(sed -n 's/^CLI=//p' "$LP0001_FAUCET_ENV"); FW=$(sed -n 's/^FAUCET_WALLET=//p' "$LP0001_FAUCET_ENV")
  FACC=$(sed -n 's/^FAUCET_ACCOUNT=//p' "$LP0001_FAUCET_ENV"); FMIN=$(sed -n 's/^MIN_BALANCE=//p' "$LP0001_FAUCET_ENV")
  AMT=${LP0001_FAUCET_AMOUNT:-120000000}
  fw() { env LEE_WALLET_HOME_DIR="$FW" LEE_WALLET_PASSWORD= LEZ_BRIDGE_SEQUENCER="$RPC" "$FCLI" wallet "$@"; }
  BEFORE=$(fw account get --account-id "Public/$FACC" 2>/dev/null | grep -oE "^Balance [0-9]+" | awk '{print $2}')
  [ -n "$BEFORE" ] && [ "$BEFORE" -ge $(( AMT + 10 * FMIN )) ] || { VERDICT=FAIL; note "faucet holds '${BEFORE}': below amount + 10 x MIN_BALANCE, not drawing"; finish_test; exit 1; }
  run fund fw auth-transfer send --from "Public/$FACC" --to-pk "$PAY_PK" --amount "$AMT"
  AFTER=$(fw account get --account-id "Public/$FACC" 2>/dev/null | grep -oE "^Balance [0-9]+" | awk '{print $2}')
  echo "$(date -u +%FT%TZ) faucet Public/$FACC -> pay Public/$PAY (pk $PAY_PK) amount $AMT rc $(cat "$O/fund.rc") $(grep -oE '[0-9a-f]{64}' "$O/fund.out" | head -1) $(grep -oE 'included in block [0-9]+' "$O/fund.out" | head -1); faucet balance $BEFORE -> $AFTER" \
    | tee -a "$G/run/FAUCET-TRANSFERS.log" | tee -a "$LOG"
  [ "$(cat "$O/fund.rc")" = 0 ] || stop fund "pay is unfunded"
fi
run bal-pay $W account get --account-id "Public/$PAY"
has bal-pay "Balance [1-9]" || stop bal-pay "pay holds nothing"
# 2. public: acting accounts and the collection
for l in actA actB; do
  run "bal0-$l" $W account get --account-id "Public/$(acct $l)"
  has "bal0-$l" "Balance [1-9]" || { run "fund-$l" $W auth-transfer send --from pay --to "Public/$(acct $l)" --amount 10000000 || stop "fund-$l" "$l is unfunded"; }
done
DEF=$(acct defn)
run def-read $CLI nft read --account defn
if ! grep -q "Definition" "$O/def-read.out"; then
  run def $CLI nft new-definition --definition defn --master mstr --name "LP0001 rc3" --supply 8 --payer pay || stop def "no collection"
fi
note "collection $DEF"
[ "${SETUP_PUBLIC_ONLY:-0}" = 1 ] && { note "public part done (SETUP_PUBLIC_ONLY)"; finish_test; exit $?; }
# 3. shield pay -> h1 (real proof 1)
run bal0-h1 $W account get --account-id "Private/$H1"
if ! has bal0-h1 "Balance [1-9]"; then
  prove_run shield-h1 $W auth-transfer send --from pay --to "Private/$H1" --amount ${LP0001_H1_AMOUNT:-50000000} || stop shield-h1 "h1 is unfunded"
  run bal-h1 $W account get --account-id "Private/$H1"
  has bal-h1 "Balance [1-9]" || stop bal-h1 "h1 holds nothing after the shield"
fi
# 4. holding and copy (real proofs 2 and 3)
run read0-h1 $CLI nft read --account h1
if ! has read0-h1 "owned: true"; then
  has read0-h1 "Copy(" || { prove_run init-h1 $CLI nft init-holding --definition $DEF --from h1 --payer h1 || stop init-h1 "h1 has no holding"; }
  prove_run print-h1 $CLI nft print --from mstr --to h1 --payer h1 || stop print-h1 "h1 owns no copy"
fi
run read-h1 $CLI nft read --account h1
expect_text read-h1 "owned: true"
run bal-end $W account get --account-id "Private/$H1"
note "h1 $(grep -m1 -oE '^Balance [0-9]+' "$O/bal-end.out")"
finish_test
