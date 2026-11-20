# Reproducible demo

The ordered commands from `RUN-AFTER-3AM.txt` and `tests/onchain/`, with what each should print.
Written 2026-11-20. **Nothing below has run yet**: the chain steps have not been run, and the real
proof has not been made. Copy one line at a time. Stop at the first failure; do not continue past
a failed check.

Tags: **[laptop] = no chain**, **[chain] = needs the throwaway chain on 3310**,
**[proving] = needs real proving (owner's word only)**. Steps 1, 2 and 11 need no chain. Steps 3-10,
12 and 13 need the chain. Only step 13 makes a real proof.

Across every step, `L` is the laptop and `H` the build host (`ssh build-host`); `$G` is
`$BUILD_ROOT/lp0001/gate` on H. The chain, `r0vm` and the operator wallet belong to
job-a (read-only). No `r0vm` on any PATH; simulation only through `tests/onchain/env.sh`. All
results land in `$G/run/RESULTS.txt` and `$G/run/T*/`. Builds on H go through `mini-lock.sh`.

## Preconditions — [chain]

Check each before step 1. Stop if one fails.

```sh
ssh build-host 'curl -s -X POST http://127.0.0.1:3310/ -H content-type:application/json -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getLastBlockId\",\"params\":[]}"'   # a block id: chain up
ssh build-host 'ls -la $BUILD_ROOT/lp0001/job-a/tools/r0vm; command -v r0vm; df -h / | tail -1'   # r0vm present in job-a only, not on PATH; >15 GB free
ssh build-host 'ls $BUILD_ROOT/dev/fix-names/.heavy.lock 2>/dev/null; pgrep -fl "prove-seqsig|outbound_host|r0vm|gate-cli-prove"'   # lock free, nothing proving
ls -d $BUILD_ROOT/target   # laptop drive mounted
```

You should see a block id, `r0vm` listed only under job-a with an empty `command -v`, no lock file
and no proving processes, and the laptop target directory.

## 1. Unit tests, vectors, JS — [laptop]

```sh
cd $SRC_ROOT/lez-nft-gate && tests/onchain/T00-unit.sh
```

About two minutes warm. You should see the vectors check pass, `cargo test --workspace` pass and
the three `node --test` suites pass.

## 2. Build and copy gate-cli — [laptop]

```sh
$SRC_ROOT/lez-nft-gate/tests/onchain/00-laptop-sync-and-build-client.sh
```

Ten to twenty minutes cold. You should see a sha256 of the client binary and the first three lines
of `gate-cli --help` printed from H: the binary was copied to `$G/out/gate-cli` and the repo to
`$G/repo` without `target`, `node_modules` or `.git`.

## 3. Build the gate guests — [chain]

```sh
ssh build-host '$BUILD_ROOT/dev/fix-names/mini-lock.sh $BUILD_ROOT/lp0001/gate/repo/tests/onchain/01-build-gate-guests.sh > $BUILD_ROOT/lp0001/gate/run/build-gate.log 2>&1; tail -4 $BUILD_ROOT/lp0001/gate/run/build-gate.log'
```

You should see two lines, "gate: … program id …" and "gate-unsafe-test: …". A guest build without
`LP0001_NFT_PROGRAM` refuses to compile — that is by design.

## 4. Wallet, funding, collection, holdings — [chain]

Owner's OK needed: one public transfer of 2,000,000,000 native from job-a's operator wallet. The
script stops without the variable.

```sh
ssh build-host 'cd $BUILD_ROOT/lp0001/gate/repo/tests/onchain && OWNER_OK_FUND_FROM_OPERATOR=1 ./02-setup-wallet.sh'
```

You should see "SETUP PASS": h1 h3 h4 h6 read as owned, h2 h5 as empty.

## 5. Deploy the gate immutable — [chain]

```sh
ssh build-host '$BUILD_ROOT/lp0001/gate/repo/tests/onchain/03-deploy-gate.sh'
```

You should see "DEPLOY-GATE PASS", "immutable true", and no "differs from spel program-id" note.
If that note appears, compare byte order before trusting pre-check 4.

## 6. Build the consumer and T4 — [chain]

```sh
ssh build-host '$BUILD_ROOT/dev/fix-names/mini-lock.sh $BUILD_ROOT/lp0001/gate/repo/tests/onchain/04-build-consumer-and-t4.sh > $BUILD_ROOT/lp0001/gate/run/build-consumer.log 2>&1; tail -3 $BUILD_ROOT/lp0001/gate/run/build-consumer.log'
```

You should see a size, sha256 and program id line for each of the two binaries, and the build log
must contain no "STOP: r0vm on PATH".

## 7. Deploy the consumer and hostile program — [chain]

```sh
ssh build-host '$BUILD_ROOT/lp0001/gate/repo/tests/onchain/05-deploy-consumer-and-t4.sh'
```

You should see the completed `gate-config.json` printed: its `consumer` and `t4_hostile` fields hold
the two deployed program ids.

## 8. T3 first — [chain]

```sh
ssh build-host '$BUILD_ROOT/lp0001/gate/repo/tests/onchain/T03-unauthorized-root.sh'
```

STOP if it fails. PASS is a refusal with "gate: root not authorized" and a control where the same
shape on the unsafe gate applies. PASS-BY-LEZ is LEZ refusing first (recorded text).

## 9. T6 second — [chain]

```sh
ssh build-host '$BUILD_ROOT/lp0001/gate/repo/tests/onchain/T06-window.sh'
```

STOP if it fails. Over and not-yet-open windows are refused, as are off-grid windows and
`ttl_class 3`; a normal proof carries `[not_before, expires_at)`. The result records which layer
refused.

## 10. The rest, T4 last — [chain]

```sh
ssh build-host '$BUILD_ROOT/lp0001/gate/repo/tests/onchain/run-all.sh'
```

This repeats T3 and T6 first (fine to run it alone instead of steps 8 and 9). T8 waits up to about
eleven minutes for a window to close. You should see T1 T2 T5 T7 T8 T9 T10 T11 T12 T13 T14 then T4,
each with a verdict in `$G/run/RESULTS.txt`: T1 the fact with no holder/payer/collection/secret/
context in the clear; T2 NOT_A_MEMBER in simulation; T5 the separate private gas payer or its exact
refusal; T7 the first context BINDING_MISMATCH; T8 the once-only Act, a late Act refused and the gas
figure the prize asks for; T9 L2 documented; T10 agreement of consumer and library; T11 linkability
windows; T12 UpdateHeader refused; T13 one of two concurrent spends refused; T14 the read-only
timestamp scan; T4 the outcome check stopping a holder-authority spend.

A "CLIENT-RULE" note on T3 or T6 means the client's outcome check refused a correct proof: fix the
rule in `crates/gate_wallet_rules`, not the design. The same applies to T1's fail-closed check.

## 11. wasm build and JS/wasm agreement — [laptop]

Adding the wasm32 target is an install on the drive — owner's OK.

```sh
RUSTUP_HOME=$BUILD_ROOT/rustup rustup target add wasm32-unknown-unknown --toolchain 1.98.1-aarch64-apple-darwin
cd $SRC_ROOT/lez-nft-gate && the build wrapper cargo +1.98.1 build -p gate_wasm --release --target wasm32-unknown-unknown
cp $BUILD_ROOT/target/lez-nft-gate/wasm32-unknown-unknown/release/gate_wasm.wasm js/gate-verify/gate_wasm.wasm
(export NVM_DIR=$HOME/.nvm; . $NVM_DIR/nvm.sh; cd js/gate-verify && node --test test/*.test.mjs)   # the wasm test stops skipping
```

You should see the wasm file copied and the `gate-verify` tests pass with the wasm agreement test
running rather than skipped.

## 12. Build the proving client — [chain]

```sh
ssh build-host '$BUILD_ROOT/dev/fix-names/mini-lock.sh $BUILD_ROOT/lp0001/gate/repo/tests/onchain/90-build-cli-prove.sh > $BUILD_ROOT/lp0001/gate/run/build-cli-prove.log 2>&1'
```

Tens of minutes. You should see a sha256 of `$G/out/gate-cli-prove`.

## 13. The one real proof — [chain, proving]

**Guarded. Owner's word only. Not run.** Preconditions above plus: Ollama models on H unloaded
(the prover uses about 9.4 GB and a model will not also fit), and the owner's decision on whether
`RISC0_PROVER=local` with Metal kernels compiled in counts as "in-process CPU prover" (F §11).

```sh
ssh build-host 'nohup $BUILD_ROOT/dev/fix-names/mini-lock.sh $BUILD_ROOT/lp0001/gate/repo/tests/onchain/91-real-proof.sh > $BUILD_ROOT/lp0001/gate/run/real-proof-console.log 2>&1 < /dev/null &'
```

Record the wall time, peak memory (`time.log`, highest `footprint_kb`), transaction size
(`decode-tx` borsh_len), machine line, and whether the dev-mode sequencer accepted the real
receipt. Then repeat T1's decode with this transaction (T01's search) and T11 with this transaction
as "same2". Until this step runs, the "about 10 minutes and 9.5 GB" line stays an estimate from
bridge claims.

## After

Leave the chain to job-a's owner — do not stop it. Delete `$G/wallet-t13` if T13 left it. Nothing
is pushed anywhere.
