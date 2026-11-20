# How to run the LP-0001 performance harness

This covers `docs/REQUIREMENTS-MAP.md` rows **31** (proof generation on consumer hardware) and
**32** (on-chain verification cost). The harness is written but **not run** by this agent: a real
proof and a chain test are the owner's call.

## Guards

- Never run on the laptop directly. Use the build host and the build lock, as in
  `tests/onchain/91-real-proof.sh` and `AGENTS.md`.
- `RISC0_DEV_MODE` and `RISC0_SERVER_PATH` must be unset for the proof; `RISC0_PROVER=local`,
  `RAYON_NUM_THREADS=4`, no `r0vm` on PATH.
- The harness refuses to start unless `BENCH_OWNER_OK=1`. It still does **not** start a proof by
  itself: it reads the log produced by the already-guarded `91-real-proof.sh`.
- The memory watchdog in `91-real-proof.sh` (kill above 11 GB or under 15% free) stays in force.

## Steps (owner, on the build host)

1. Build the prove CLI: `tests/onchain/90-build-cli-prove.sh`.
2. With the owner's word and the lock, run `tests/onchain/91-real-proof.sh`. It writes
   `$G/run/real-proof-<UTC>/time.log` and `sampler.log`.
3. Run `tests/onchain/T08-consumer.sh` on the throwaway chain. It records
   `Act gas: fee <n>` in `$G/run/T08-consumer/log.txt`.
4. From the repo root on the build host:

   ```sh
   BENCH_OWNER_OK=1 bash tests/onchain/BENCH-proof-and-gas.sh
   ```

5. Commit the regenerated `tests/onchain/bench-results.csv` and
   `docs/lp0001/BENCHMARK-RESULTS.md` together. Do not edit the numbers by hand.

## What is recorded

`bench-results.csv` is append-only, one row per metric: UTC timestamp, requirement row, metric
name, value, unit, source log and notes. The harness regenerates the markdown table from it.
Proof wall time comes from the `real` line of `time.log`; peak footprint from the sampled
`footprint_kb` maximum; the Act cost from the last `fee <n>` in T08's log. If a source is missing,
the harness prints a STOP message and writes no number.
