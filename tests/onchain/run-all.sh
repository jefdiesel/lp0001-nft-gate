#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# BUILD HOST. The on-chain tests in the spec's order: T3 and T6 first (stop if either fails),
# then the rest; T4 last. Results: $G/run/RESULTS.txt.
D=$(dirname "$0")
. "$D/env.sh"
"$D/T03-unauthorized-root.sh" || { echo "T3 failed: STOP (spec §8.1)"; exit 1; }
"$D/T06-window.sh" || { echo "T6 failed: STOP (spec §8.1)"; exit 1; }
for t in T01-holder-proves T02-non-member T05-gas-payer T07-reprove T08-consumer T09-transfer-then-prove \
         T10-agreement T11-linkability T12-immutable T13-collision T04-hostile; do
  "$D/$t.sh" || echo "($t did not pass; continuing)"
done
cat "$G/run/RESULTS.txt"
