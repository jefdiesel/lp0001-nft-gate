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
# 2. OWNER OK: fund `pay` from the job-a operator wallet (public transfer, 2 billion native).
if [ "${OWNER_OK_FUND_FROM_OPERATOR:-0}" = 1 ]; then
  run fund env LEE_WALLET_HOME_DIR=$J/chain-home/wallets/operator $J/out/nft-drive-v2 wallet \
      auth-transfer send --from operator --to "Public/$PAY" --amount 2000000000
else
  note "skipped funding: set OWNER_OK_FUND_FROM_OPERATOR=1 after the owner agrees"; finish_test; exit 1
fi
# 3. sealed funding of each private account (spec §4.3: never --to <label>), then receive it
fund_private() { # label amount
  local id keys npk vpk pos
  id=$(acct $1); keys=$($W account show-keys --account-id "Private/$id" 2>/dev/null)
  npk=$(echo "$keys" | sed -n 1p); vpk=$(echo "$keys" | sed -n 2p)
  run "seal-$1" $W auth-transfer send --from pay --to-npk "$npk" --to-vpk "$vpk" --amount $2
  run "pending-$1" $W pending list
  pos=$(grep -oE "^Message [0-9]+" "$O/pending-$1.out" | tail -1 | grep -oE "[0-9]+")
  [ -n "$pos" ] && run "receive-$1" $W pending receive --position "$pos"
}
for l in h1 h2 h3 h4 h5 h6 gas; do fund_private $l 60000000; done
for l in actA actB atk; do run "fund-$l" $W auth-transfer send --from pay --to "Public/$(acct $l)" --amount 50000000; done
# 4. the collection: supply 8 (the master prints 7)
run def $CLI nft new-definition --definition defn --master mstr --name "LP0001 test" --supply 8 --payer pay
DEF=$(acct defn)
for l in h1 h2 h3 h4 h5 h6; do run "init-$l" $CLI nft init-holding --definition $DEF --from $l --payer $l; done
run init-atk $CLI nft init-holding --definition $DEF --from atk --payer pay
for l in h1 h3 h4 h6; do run "print-$l" $CLI nft print --from mstr --to $l --payer pay; done
run pending-nft $CLI nft pending
for l in h1 h2 h3 h4 h5 h6 atk; do run "read-$l" $CLI nft read --account $l; done
note "collection $DEF; h1 h3 h4 h6 should read ...01 (owned), h2 h5 atk ...00"
finish_test
