# LP-0001 measured performance — rows 31 and 32

No numbers are committed yet. This file is the stable home for the results table; the harness
`tests/onchain/BENCH-proof-and-gas.sh` rewrites it from `bench-results.csv` when the owner runs it
on the build host.

| Row | Metric | Value | Source |
| --- | --- | --- | --- |
| 31 | Proof generation wall time | not measured | guarded `91-real-proof.sh` log |
| 31 | Peak proof footprint | not measured | guarded `91-real-proof.sh` sampler |
| 32 | On-chain Act gas | not measured | `tests/onchain/T08-consumer.sh` → `fee` |
| 32 | T08 verdict | not run | `tests/onchain/T08-consumer.sh` |

Machine and OS are recorded by the harness when it runs. The on-chain figure is a `fee` value from
`decode-tx`, not an EVM gas number: LEZ reports transaction cost as a fee, so the table labels the
unit `fee units` to avoid inventing a CU conversion.
