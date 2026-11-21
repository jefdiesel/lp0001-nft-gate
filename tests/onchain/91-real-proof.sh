#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# THE ONE REAL PROOF of `Prove` (spec §8.2). NOT RUN by this script alone: the owner starts it
# only when RUN-AFTER-3AM.txt's preconditions hold and no other proof is running (there is no lock
# wrapper now; the caller enforces one heavy proof at a time). RUNS ON THE BUILD HOST.
# In-process CPU prover, 4 threads, never r0vm; kill above 11 GB footprint or under 15% free
# (the same sampler as job-a/run-real-proof.sh). Records time, peak memory, tx size.
set -u
: "${LP0001_GATE_HOME:?set LP0001_GATE_HOME to this run folder on the build host}"
G=$LP0001_GATE_HOME
. "$G/repo/tests/onchain/env.sh"
# dev mode OFF and no r0vm server for THIS command only:
unset RISC0_DEV_MODE RISC0_SERVER_PATH
export RISC0_PROVER=local RAYON_NUM_THREADS=4
if command -v r0vm >/dev/null 2>&1; then echo "STOP: r0vm on PATH"; exit 1; fi
[ -x $G/out/gate-cli-prove ] || { echo "STOP: build gate-cli-prove first (90-build-cli-prove.sh)"; exit 1; }
LOG=$G/run/real-proof-$(date -u +%Y%m%dT%H%MZ); mkdir -p "$LOG"
N=$(openssl rand -hex 32); echo "$N" > "$LOG/app-nonce"
# PROVE_HOLDER (default h6) proves; PROVE_ONCHAIN_ACTOR set = an on-chain context for that actor's
# next Act at the consumer (tests/onchain/rc3/E2E-rc3.sh), else the off-chain context above.
HOLDER=${PROVE_HOLDER:-h6}
if [ -n "${PROVE_ONCHAIN_ACTOR:-}" ]; then CTX=(--onchain-actor "$PROVE_ONCHAIN_ACTOR")
else CTX=(--caller-identity real-proof --purpose "real proof" --app-nonce "$N"); fi
echo "holder $HOLDER context ${CTX[*]}" > "$LOG/request.txt"
/usr/bin/time -l $G/out/gate-cli-prove prove --holder "$HOLDER" --payer "$HOLDER" "${CTX[@]}" \
    --out "$LOG/proof.json" > "$LOG/client.log" 2> "$LOG/time.log" &
TPID=$!
sleep 2
PID=$(pgrep -P $TPID gate-cli-prove || echo $TPID)
echo "time pid $TPID, client pid $PID" | tee "$LOG/sampler.log"
LIMIT_KB=$((11*1024*1024))
while kill -0 $PID 2>/dev/null; do
  FP=$(/usr/bin/footprint -p $PID 2>/dev/null | awk '/Footprint:/ {for(i=1;i<=NF;i++) if($i=="Footprint:"){v=$(i+1); u=$(i+2)}; if (u=="GB") v*=1048576; else if (u=="MB") v*=1024; print int(v); exit}')
  RSS=$(ps -o rss= -p $PID | tr -d ' ')
  FREE=$(memory_pressure -Q | awk -F': ' '/free percentage/ {gsub("%","",$2); print $2}')
  echo "$(date -u +%H:%M:%S) footprint_kb=${FP:-0} rss_kb=${RSS:-0} free_pct=${FREE:-?}" >> "$LOG/sampler.log"
  if [ "${FP:-0}" -gt "$LIMIT_KB" ] || [ "${FREE:-100}" -lt 15 ]; then
    echo "KILL: footprint_kb=$FP rss_kb=$RSS free_pct=$FREE" | tee -a "$LOG/sampler.log"
    kill -TERM $PID; sleep 5; kill -KILL $PID 2>/dev/null
    break
  fi
  sleep 5
done
wait $TPID; echo "exit $?" >> "$LOG/sampler.log"
tail -8 "$LOG/client.log"; grep -E "real|maximum resident|peak memory" "$LOG/time.log"
grep footprint_kb "$LOG/sampler.log" | sort -t= -k2 -n | tail -1
H=$(grep -oE "^submitted [0-9a-f]{64}" "$LOG/client.log" | tail -1 | cut -d' ' -f2)
[ -n "$H" ] && RISC0_DEV_MODE=1 RISC0_SERVER_PATH=$LP0001_R0VM $CLI decode-tx --hash $H | head -3
echo "machine: $(sysctl -n machdep.cpu.brand_string) $(( $(sysctl -n hw.memsize) / 1073741824 )) GiB $(sw_vers -productVersion)"
