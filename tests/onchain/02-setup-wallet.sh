#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# BUILD HOST. A throwaway wallet in $G/wallet, a collection, and holdings for every test.
# Accounts: pay (public, funds and deploys), defn + mstr (public, the collection), actA, actB
# (public acting accounts), atk (public, T4's attacker holding), h1 h3 h4 h6 (private, own a
# copy), h2 (private, initialised holding, owns nothing), h5 (private, fresh, T9), gas
# (private fee payer, T5).
#
# OWNER OK NEEDED for step 2: the only way to fund this wallet is one transfer from the
# throwaway chain's operator wallet in job-a (a folder this work was given read-only).
set -uo pipefail
. "$(dirname "$0")/env.sh"
. "$(dirname "$0")/lib.sh" SETUP
W="$CLI wallet"
# 1. wallet, pinned to the local chain
[ -e "$G/wallet/storage.json" ] || run new $W account new public --label pay
run net $W change-network http://127.0.0.1:3310
for l in defn mstr actA actB atk; do [ -n "$(acct $l)" ] || run "new-$l" $W account new public --label $l; done
for l in h1 h2 h3 h4 h5 h6 gas; do [ -n "$(acct $l)" ] || run "new-$l" $W account new private --label $l; done
PAY=$(acct pay)
# stop NAME WHY: a step everything after depends on failed; record it and stop here, so the
# summary names the cause instead of the empty reads it leads to.
stop() { VERDICT=FAIL; note "$1 failed (exit $(cat "$O/$1.rc")): $(grep -v '^\$ ' "$O/$1.out" | grep -m1 -iE 'error|fail|panic|revert' | cut -c1-300); $2"; finish_test; exit 1; }
# 2. OWNER OK: fund `pay` from the job-a operator wallet (public transfer, 2 billion native).
# `pay` is a never-used account of ANOTHER wallet: LEZ 0.4 credits such an account only with its
# public key as evidence ("admission"), so the transfer names it by --to-pk. `--to Public/<id>`
# reverts in simulation ("declared public but not admitted") and is not sent, which left `pay`
# at 0 and failed every later step (run of 2026-11-19 15:56Z).
PAY_PK=$($W account get --keys --account-id "Public/$PAY" 2>/dev/null | awk '/^pk /{print $2}')
[ ${#PAY_PK} = 64 ] || { VERDICT=FAIL; note "no public key for pay (got '$PAY_PK')"; finish_test; exit 1; }
if [ "${OWNER_OK_FUND_FROM_OPERATOR:-0}" = 1 ]; then
  run fund env LEE_WALLET_HOME_DIR=$J/chain-home/wallets/operator $J/out/nft-drive-v2 wallet \
      auth-transfer send --from operator --to-pk "$PAY_PK" --amount 2000000000 || stop fund "pay is unfunded"
else
  note "skipped funding: set OWNER_OK_FUND_FROM_OPERATOR=1 after the owner agrees"; finish_test; exit 1
fi
run bal-pay $W account get --account-id "Public/$PAY"
has bal-pay "Balance [1-9]" || stop bal-pay "pay still holds nothing after the funding transfer"
# 3. sealed funding of each private account (spec §4.3: never --to <label>), then receive it
fund_private() { # label amount
  local id keys npk vpk pos
  id=$(acct $1); keys=$($W account show-keys --account-id "Private/$id" 2>/dev/null)
  npk=$(echo "$keys" | sed -n 1p); vpk=$(echo "$keys" | sed -n 2p)
  run "seal-$1" $W auth-transfer send --from pay --to-npk "$npk" --to-vpk "$vpk" --amount $2 || stop "seal-$1" "$1 is unfunded"
  run "pending-$1" $W pending list
  pos=$(grep -oE "^Message [0-9]+" "$O/pending-$1.out" | tail -1 | grep -oE "[0-9]+")
  [ -n "$pos" ] || { VERDICT=FAIL; note "pending-$1 lists no 'Message N' for the sealed funding of $1"; finish_test; exit 1; }
  run "receive-$1" $W pending receive --position "$pos" || stop "receive-$1" "$1 did not receive its funding"
}
for l in h1 h2 h3 h4 h5 h6 gas; do fund_private $l 60000000; done
for l in actA actB atk; do run "fund-$l" $W auth-transfer send --from pay --to "Public/$(acct $l)" --amount 50000000 || stop "fund-$l" "$l is unfunded"; done
# 4. the collection: supply 8 (the master prints 7)
run def $CLI nft new-definition --definition defn --master mstr --name "LP0001 test" --supply 8 --payer pay || stop def "no collection"
DEF=$(acct defn)
for l in h1 h2 h3 h4 h5 h6; do run "init-$l" $CLI nft init-holding --definition $DEF --from $l --payer $l || stop "init-$l" "$l has no holding"; done
run init-atk $CLI nft init-holding --definition $DEF --from atk --payer pay || stop init-atk "atk has no holding"
# A print to a private holding is a proven transaction: its payer must be private (a public
# payer fails there, F §4 "Accounts key not found"), so the recipient pays.
for l in h1 h3 h4 h6; do run "print-$l" $CLI nft print --from mstr --to $l --payer $l || stop "print-$l" "$l owns no copy"; done
run pending-nft $CLI nft pending
for l in h1 h2 h3 h4 h5 h6 atk; do run "read-$l" $CLI nft read --account $l; done
for l in h1 h3 h4 h6; do expect_text "read-$l" "owned: true"; done
for l in h2 h5 atk; do expect_text "read-$l" "owned: false"; done
expect_ok def
note "collection $DEF"
finish_test
