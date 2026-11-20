#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Offline self-test for BENCH-proof-and-gas.sh: fake real-proof and T08 logs in a temp G,
# run the harness against them, check the CSV and the table. Starts no proof, needs no chain.
set -eu
# A clean PATH: the harness refuses to run while r0vm is on PATH.
HERE=$(cd "$(dirname "$0")" && pwd)
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
G=$T/g
mkdir -p "$G/run/real-proof-20261120T1200Z" "$G/run/T08"
cat > "$G/run/real-proof-20261120T1200Z/time.log" <<'L'
      458.31 real      1702.11 user        23.40 sys
          9800000000  maximum resident set size
L
cat > "$G/run/real-proof-20261120T1200Z/sampler.log" <<'L'
12:00:01 footprint_kb=1200000 rss_kb=900000 free_pct=70
12:03:01 footprint_kb=9400123 rss_kb=6500000 free_pct=22
12:06:01 footprint_kb=8100000 rss_kb=6000000 free_pct=30
L
cat > "$G/run/T08/log.txt" <<'L'
Act gas: fee 1238056
T08 PASS
L
printf 'G=%s\n' "$G" > "$T/env.sh"
PATH=/usr/bin:/bin:/usr/sbin:/sbin BENCH_OWNER_OK=1 BENCH_ENV="$T/env.sh" BENCH_CSV="$T/out.csv" BENCH_MD="$T/out.md" \
  bash "$HERE/BENCH-proof-and-gas.sh" > "$T/run.out"
fail=0
check() { if grep -qF -- "$2" "$1"; then echo "ok: $2"; else echo "FAIL: missing '$2' in $(basename "$1")"; fail=1; fi; }
check "$T/out.csv" ",31,proof_wall_time,458.31,s,"
check "$T/out.csv" ",31,proof_peak_footprint,9400123,KiB,"
check "$T/out.csv" ',32,act_gas,1238056,fee-units,'
check "$T/out.csv" 'T08 verdict: PASS'
check "$T/out.md" "| 31 | Proof generation wall time | 458.31 s |"
check "$T/out.md" "| 32 | T08 verdict | PASS |"
# Refuses without the owner's switch, and never starts a proof.
if PATH=/usr/bin:/bin:/usr/sbin:/sbin BENCH_ENV="$T/env.sh" bash "$HERE/BENCH-proof-and-gas.sh" >/dev/null 2>&1; then echo "FAIL: ran without BENCH_OWNER_OK"; fail=1; else echo "ok: refuses without BENCH_OWNER_OK"; fi
# Starts no proof: no line outside comments and echo text runs a prover, the gate CLI or cargo.
if grep -vE '^\s*#|echo |command -v r0vm' "$HERE/BENCH-proof-and-gas.sh" | grep -nE '91-real-proof|90-build|gate-cli|\$CLI|r0vm|cargo '; then echo "FAIL: harness may start a proof"; fail=1; else echo "ok: harness starts no proof"; fi
exit $fail
