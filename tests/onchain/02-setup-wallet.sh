#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# BUILD HOST. A throwaway wallet in $G/wallet, a collection, and holdings for every test.
# Accounts: pay (public, funds and deploys), defn + mstr (public, the collection), actA, actB
# (public acting accounts), atk (public, T4's attacker holding), h1 h3 h4 h6 (private, own a
# copy), h2 (private, initialised holding, owns nothing), h5 (private, fresh, T9), gas
# (private fee payer, T5).
#
# OWNER OK NEEDED for steps 2 and 3: the only way to fund this wallet is from the throwaway
# chain's operator wallet in job-a (a folder this work was given read-only): one public transfer
# to `pay` and one sealed send to `gas`, whose fee one of the operator wallet's private accounts pays.
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
# 3. sealed funding of each private account (spec §4.3: never --to <label>), then receive it.
# A sealed send is a privacy-preserving transaction, and its fee can only come from a private
# account of the SIGNING wallet that holds the fee reserve (about 5 million); a public sender
# cannot pay it ("no private account to pay its fee", run of 2026-11-19 17:15Z). This wallet
# has no funded private account yet, so `gas` is bootstrapped by the operator wallet, paid by one
# of the operator wallet's own private accounts (job-a's proven path, run/a2w-23a.sh), and then
# pays the fee of every other seal from `pay`. A receive is paid by the receiving account from
# the credit it claims, so it needs no other payer.
# OWNER OK covers this one sealed send from the operator wallet. Default payer: pask2 (DBFV...),
# not ph1/ph2 (job-a's real-proof pair).
OPERATOR_FEE_PAYER=${OPERATOR_FEE_PAYER:-DBFVgupPtnCJrTF9LwQ9FnKyuAXYasL4kzdrz72pefTA}
keys_of() { $W account show-keys --account-id "Private/$(acct $1)" 2>/dev/null; }
receive_private() { # label
  local pos
  run "pending-$1" $W pending list
  # the credit addressed to this account (a fee refund to another account's alias may be listed too)
  pos=$(grep -E "^Message [0-9]+: to $(acct $1) " "$O/pending-$1.out" | tail -1 | grep -oE "^Message [0-9]+" | grep -oE "[0-9]+")
  [ -n "$pos" ] || { VERDICT=FAIL; note "pending-$1 lists no 'Message N: to <$1>' for the sealed funding of $1"; finish_test; exit 1; }
  run "receive-$1" $W pending receive --position "$pos" || stop "receive-$1" "$1 did not receive its funding"
  run "bal-$1" $W account get --account-id "Private/$(acct $1)"
  has "bal-$1" "Balance [1-9]" || stop "bal-$1" "$1 holds nothing after its receive"
}
keys=$(keys_of gas); npk=$(echo "$keys" | sed -n 1p); vpk=$(echo "$keys" | sed -n 2p)
[ ${#npk} = 64 ] || { VERDICT=FAIL; note "no npk for gas (got '$npk')"; finish_test; exit 1; }
run seal-gas env LEE_WALLET_HOME_DIR=$J/chain-home/wallets/operator $J/out/nft-drive-v2 wallet \
    --fee-payer "$OPERATOR_FEE_PAYER" auth-transfer send --from operator --to-npk "$npk" --to-vpk "$vpk" \
    --amount 200000000 || stop seal-gas "gas is unfunded"
receive_private gas
GAS=$(acct gas)
fund_private() { # label amount: sealed from pay, fee paid by gas (a private account of this wallet)
  local keys npk vpk
  keys=$(keys_of $1); npk=$(echo "$keys" | sed -n 1p); vpk=$(echo "$keys" | sed -n 2p)
  run "seal-$1" $W --fee-payer "$GAS" auth-transfer send --from pay --to-npk "$npk" --to-vpk "$vpk" --amount $2 || stop "seal-$1" "$1 is unfunded"
  receive_private $1
}
# Every private transaction costs about 5 million in fees and needs a reserve of about 8.4
# million at send time. h1 pays for its receive, init, print and the Proves of T01, T07, T08
# (two) and T13 plus T13's send: 60 million ran out at T08's second Prove (run of
# 2026-11-19, FEE_RESERVE "holds 5613277, the reserve is 8396219").
fund_private h1 300000000
for l in h2 h3 h4 h5 h6; do fund_private $l 150000000; done
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
