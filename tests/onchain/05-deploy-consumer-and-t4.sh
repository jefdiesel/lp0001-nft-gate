#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# BUILD HOST. Deploys the consumer and the TEST ONLY hostile program; completes the config.
# The spec fixes no header rule for the consumer; it is deployed mutable here (see
# docs/OWNER-DECISIONS.md, note after Q1).
set -uo pipefail
. "$(dirname "$0")/env.sh"
. "$(dirname "$0")/lib.sh" DEPLOY-CONSUMER
PAY=$(acct pay)
run deploy-consumer "$SPEL" deploy "$G/out/consumer.bin" --fee-payer "$PAY"
CONSUMER=$(tail -1 "$O/deploy-consumer.out" | tr -d '[:space:]')
HOSTILE=""
if [ "${LP0001_NO_TEST_PROGRAMS:-0}" != 1 ]; then
  run deploy-hostile "$SPEL" deploy "$G/out/t4-hostile.bin" --fee-payer "$PAY"
  HOSTILE=$(tail -1 "$O/deploy-hostile.out" | tr -d '[:space:]')
fi
python3 -I - "$GATE_CONFIG" <<PY
import json,sys
c=json.load(open(sys.argv[1])); c["consumer"]="$CONSUMER"; c["t4_hostile"]="$HOSTILE"
json.dump(c,open(sys.argv[1],"w"),indent=1)
PY
cat "$GATE_CONFIG" | tee -a "$LOG"
finish_test
