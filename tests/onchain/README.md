# On-chain tests: settings and where each script runs

Every machine-specific path is a setting. No script has a private default; an unset required
setting stops the script with a plain message naming it. `env.sh` is the one place that defines the
build-host settings; `rc3/env-rc3.sh` adds the public-chain ones. The scripts do **not** use ssh:
run each one on the machine its header names, with that machine's folders mounted where needed.
There is no build lock or build wrapper: the caller must not run another heavy build or proof at
the same time where the old lock used to serialize them.

## Settings defined in `env.sh` (required)

| Setting | Meaning |
| --- | --- |
| `LP0001_GATE_HOME` | This run's own folder on the build host: `out/`, `wallet/`, `run/`, `guest-target/`. Exported as `G`. |
| `LP0001_REPO` | Repository root on the machine running the script. Exported as `R`. |
| `LP0001_SPEL` | The `spel` CLI binary (`deploy`, `program-id`). Exported as `SPEL`. |
| `LP0001_PACKAGER` | The `risc0-packager` binary used by the guest-build scripts. Exported as `PACK`. |
| `LP0001_SEQ_RPC` | JSON-RPC URL of the chain under test. Exported as `RPC` and `GATE_CLI_SEQUENCER`. |
| `LP0001_R0VM` | `r0vm` binary for local simulation; required only in the default dev-receipt mode. Exported as `RISC0_SERVER_PATH`. |

Optional: `LP0001_CLI` (default `$G/out/gate-cli`), `LP0001_NFT` (reference NFT program id),
`LP0001_EXTRA_PATH` (extra PATH entries, e.g. a Homebrew bin dir).

## Extra settings used by individual scripts

| Setting | Script | Meaning |
| --- | --- | --- |
| `LP0001_OPERATOR_WALLET` | `02-setup-wallet.sh` | Operator wallet directory used once to fund `pay`/`gas` (owner's call). |
| `LP0001_NFT_DRIVE` | `02-setup-wallet.sh` | The CLI binary used for that funding. |
| `LP0001_CARGO_TARGET_DIR` | `00-laptop-sync-and-build-client.sh`, `T00-unit.sh` | Cargo target dir on the laptop. |
| `LP0001_T00_LOG` | `T00-unit.sh` | Writable log file for the unit-test run. |
| `LP0001_NVM_DIR` | `T00-unit.sh` | Optional; defaults to `$HOME/.nvm`. |
| `LP0001_GUARD` | `rc3/prove-lib.sh` | Memory guard wrapper around each real proof. |
| `LP0001_FAUCET_ENV` | `rc3/02-setup-rc3.sh` | Faucet wallet's env file (read only). |

`rc3/env-rc3.sh` requires `LP0001_GATE_HOME`, `LP0001_REPO`, `LP0001_SPEL`, `LP0001_PACKAGER`,
`LP0001_GUARD`, `LP0001_FAUCET_ENV` and `LP0001_SEQ_RPC`; it sets `LP0001_REAL_PROVING=1` and
`LP0001_NO_TEST_PROGRAMS=1`. Use it through `rc3/with-rc3.sh <script> [args]`.

## Where each script runs

| Script(s) | Machine | Note |
| --- | --- | --- |
| `env.sh`, `lib.sh`, `T01`–`T15`, `02`–`05`, `BENCH-*` | build host | dev receipts by default (`RISC0_DEV_MODE=1`); no real proof |
| `00-laptop-sync-and-build-client.sh`, `T00-unit.sh` | laptop | plain cargo/node; no build wrapper |
| `90-build-cli-prove.sh`, `91-real-proof.sh` | build host | real-proving path; one heavy job at a time (no lock) |
| `rc3/*` | build host | our public rc3 chain, real receipts; set up with `with-rc3.sh` |
| `ci-run.sh` | CI runner | builds its own env and runs the dev-mode subset; see `.github/workflows/e2e.yml` |

`run-all.sh` runs the throwaway-chain tests in the spec's order. `ci-run.sh` is the CI equivalent
for a fresh local chain; both use the same settings shape.
