# LP-0001 requirements map

READ = file:line in this repo (or a test that exists here) was checked for this map.
GUESS = the work lives in another repo / on a chain I did not inspect. Status is for the
**submission as a whole**, with the repo evidence named; this repo is the gate, verifiers,
SDK shim and mini-app, not the Names wallet.

Prize text: `LP-0001.md` master (lines as in the briefing). Source labels: gate `crates/gate_program/src/lib.rs`, core `crates/gate_core/src/lib.rs`, verify `crates/gate_verify/src/lib.rs`, JS `js/gate-verify/src/rpc.mjs`, shim `js/wallet-shim/src/membership.mjs`, app `miniapp/web/`, tests `tests/onchain/T*.sh`.

## Functionality

| # | Requirement | Evidence | Status | Mark |
|---|---|---|---|---|
| 1 | Own/send/receive NFTs, public + private | Collectibles sends/receives and the module's NFT actions (`names-wallet:basecamp/qml/Main.qml:7525-7543`, `names-wallet:basecamp-module/rust-lib/src/lib.rs:1302-1306`); this gate reads NFT state (`gate:63-88`) | MET | READ |
| 2 | NFT display, grouped, metadata status | NFT cards with metadata rows (`names-wallet:basecamp/qml/Main.qml:10966`, `names-wallet:basecamp/qml/nft.mjs:40`), failed fetches shown as problems (`nft.mjs:106`) | MET | READ |
| 3 | Collection view, counts | Collection counts and grouping (`names-wallet:basecamp/qml/Main.qml:7558`, `names-wallet:basecamp/qml/nft.mjs:70`) | MET | READ |
| 4 | SDK NFT APIs: enumerate, membership, transfer proposals | Wallet module exposes holdings, membership query, transfer proposals and status (`names-wallet:basecamp-module/rust-lib/src/lib.rs:1346-1349`); shim exposes the proof request (`shim:118-160`) | MET | READ |
| 5 | Private collection-membership proof | `gate:63-196` (Prove → own NFT `ReadState` → public fact); tests `crates/gate_program/tests/conversation.rs:183-204` | MET | READ |
| 6 | On-chain verification | Reference consumer `crates/consumer_program/src/lib.rs:91-146`; `tests/onchain/T08-consumer.sh` written | PARTLY — test not run (README:15) | READ |
| 7 | Off-chain verification + library + docs | `verify:219` Rust, `js:64` JS, shared `gate_core::check` (`core:535-560`); README `js/gate-verify/README.md:22-36` | MET | READ |
| 8 | Replay resistance across contexts | Binding over collection + context (`core:162-164`); tests `properties.rs:103-149` and T07/T08 | MET | READ |
| 9 | Proof via SDK with approval prompt | Shim request/status (`shim:118-160`); wallet request + sheet (`names-wallet:basecamp-module/rust-lib/src/lib.rs:1356`, `:1328`) | MET | READ |
| 10 | Proof request stands alone | Shim request body has no account/audience (`shim:52-70`, test `js/wallet-shim/test/membership.test.mjs:28`) | PARTLY — wallet grant not here | READ |
| 11 | Token-gating mini-app, both consumptions, submission id | `miniapp/web/{index.html,app.mjs,logic.mjs}`, `miniapp/basecamp/` skeleton; `Gated` carries `SUBMISSION_ID` (`gate_interface:87-96`, consumer `:125-140`) | PARTLY — deployed nowhere yet | READ |
| 12 | Testnet 0.4 end-to-end | Only standalone 0.4 chain; README:15 "nothing has run on a chain" | NOT MET | READ |

## Usability

| # | Requirement | Evidence | Status | Mark |
|---|---|---|---|---|
| 13 | Module/SDK usable by other modules and dApps, additive | Wallet provider NFT + proof capabilities (`names-wallet:basecamp-module/rust-lib/src/provider_nft.rs:32-33`, `lib.rs:1348-1356`); shim (`shim:118-160`) | MET | READ |
| 14 | Basecamp GUI + CLI cover NFT holdings/transfers/proof | Wallet Collectibles GUI and approval sheets (`names-wallet:basecamp/qml/Main.qml:7525-7543`), CLI (`client/src/main.rs`: `nft`, `prove`, `act`, `verify`) | MET (code); not deployed | READ |
| 15 | Module catalog forked and published | No catalog files or `logos-repo.json` in this repo | NOT MET | READ |
| 16 | NFT-transfer approval prompts name dApp/account/collection/asset | Wallet NFT sheet names the dApp, account, collection and asset (`names-wallet:basecamp-module/rust-lib/src/lib.rs:1302-1317`, `Main.qml:7758-7763`) | MET | READ |
| 17 | Proof prompt distinct, states disclosures, separate approval | Proof request approved on its own sheet, not with transfers (`names-wallet:basecamp-module/rust-lib/src/provider_nft.rs:531`, `lib.rs:1328`) | MET | READ |
| 18 | Per-dApp capabilities legible and revocable | Per-app grants `nft_list`/`nft_transfers`/`membership_proof`, individually revocable (`names-wallet:basecamp-module/rust-lib/src/lib.rs:1320-1325`, `provider_nft.rs:577`) | MET | READ |
| 19 | Program source verification in approval prompt | Approval shows Program Registry verification (`names-wallet:basecamp/qml/Main.qml:9423`, `:11138`); client pre-check (`client/src/main.rs:393-401`) | MET | READ |
| 20 | UX quality, non-expert | Mini-app logic + tests (`miniapp/test/logic.test.mjs`); no deployed app | PARTLY | READ |
| 21 | Integration guide, SDK + proof + gating | `js/gate-verify/README.md`, `docs/DESIGN.md`, `docs/WALLET-INTEGRATION.md`, `docs/OWNER-DECISIONS.md` | MET (docs); deployment pending | READ |

## Reliability

| # | Requirement | Evidence | Status | Mark |
|---|---|---|---|---|
| 22 | Clear proof-failure errors | `client/src/main.rs` maps refusals via `gate_wallet_rules::classify` (`gate_wallet_rules/src/lib.rs:75-96`) | MET | READ |
| 23 | Deterministic invalid-proof outcomes; two verifiers agree | One `gate_core::check` (`core:535-560`), shared vectors `vectors/*.json`, T10 | MET | READ |
| 24 | No proof / no private enumeration without explicit approval | Per-app capability checks before NFT/proof requests (`names-wallet:basecamp-module/rust-lib/src/provider_nft.rs:32-33`, `:165`, `:531`); shim refuses caller fields (`shim:52-70`) | MET | READ |
| 25 | Privacy property + stated assumptions | `docs/DESIGN.md:103-108`, spec §6, `js/gate-verify/README.md:7-20` | MET (write-up), not proven on real receipts | READ |
| 26 | Private transfers unlinkable | Job A observation `docs/lp0001/LP0001-JOB-A-FINDINGS.md` §8b.3 (development receipts) | PARTLY — dev receipts only | READ |
| 27 | Proof freshness and invalidation | TTL menu + window (`core:30-36`, `gate:152-166`), one slot overwrite (`gate:198-211`), T07 | MET in logic; chain tests unrun | READ |
| 28 | State survives restarts; no mandatory external services; Logos Storage | Gate state on chain; no Logos Storage client in this repo | PARTLY | READ |
| 29 | Centralised media fetching optional/disableable | Per-card image fetch needs an explicit tick (`names-wallet:basecamp/qml/Main.qml:7540-7542`, `:10971-10976`) | MET | READ |
| 30 | Large metadata/media on Logos Storage | `crates/storage_client`: upload → CID, fetch via `/network/stream`, `verify_media` by `mediaSha256`; offline tests against a fake that answers like the real node (API shape checked on a real Logos Storage node 2026-11-20). Not yet used by an NFT mint/metadata flow | PARTLY | READ |

## Performance

| # | Requirement | Evidence | Status | Mark |
|---|---|---|---|---|
| 31 | Proof generation on consumer hardware; benchmarks | Harness `tests/onchain/BENCH-proof-and-gas.sh` (+ self-test), no numbers yet. No proof run: README:15, `tests/onchain/91-real-proof.sh` written, guarded by AGENTS.md:19 | NOT MET | READ |
| 32 | On-chain verification CU cost documented | `tests/onchain/T08-consumer.sh:19-20` records gas when run; no numbers yet | NOT MET | READ |

## Supportability

| # | Requirement | Evidence | Status | Mark |
|---|---|---|---|---|
| 33 | Deployed and tested on testnet 0.4 | Not live; no deployments here (README:15) | NOT MET | READ |
| 34 | mkLogosModule build | No flake/module-builder config here | NOT MET | READ |
| 35 | Linux/macOS/Windows/Android | Rust/JS tests ran on macOS only | PARTLY | READ |
| 36 | e2e tests against real sequencer in CI | Unit CI now exists (`.github/workflows/ci.yml`: `cargo test --workspace`, `node --test` in the three JS dirs); chain e2e still not in CI | PARTLY | READ |
| 37 | Reproducible demo script with real proving | `RUN-AFTER-3AM.txt` (ordered commands), `tests/onchain/91-real-proof.sh` guarded | PARTLY — real proving never run | READ |
| 38 | README setup/NFT/proof/media/fetching | `README.md` (setup, layout, trust and on-chain checks, new "NFT media and metadata" section at :54), verifier README | MET | READ |
| 39 | SDK docs mini-app worked example + capabilities | `js/gate-verify/README.md`, `docs/WALLET-INTEGRATION.md` | MET (docs) | READ |

## Adoption

| # | Requirement | Evidence | Status | Mark |
|---|---|---|---|---|
| 40 | Baseline 50 testimonials / 50 accounts | No chain submission or testimonials | NOT MET | GUESS |
| 41 | 5 independent collections deployed | None deployed (README:15) | NOT MET | GUESS |
| 42 | 3 third-party Basecamp apps using SDK | No published apps | NOT MET | GUESS |
| 43 | 100 gated actions / 100 accounts | No gated actions on the official zone | NOT MET | GUESS |
| 44 | Actions over ≥2 months, ≥20/month | No activity window started | NOT MET | GUESS |
| 45 | 20 Discord + 20 X testimonials | None; AGENTS.md:10 forbids posting | NOT MET | READ |
| 46 | macOS coverage in evidence | Tests ran on macOS; no adoption evidence | PARTLY | READ |

## Summary

MET: 1, 2, 3, 4, 5, 7, 8, 9, 13, 14, 16, 17, 18, 19, 21, 22, 23, 24, 25, 27, 29, 38, 39.
The largest gaps are deployments, real proving, module catalog/CI, the wallet-side UX, and all
adoption. This map was written from this repo, the Names Wallet worktree and the prize text; no
chain or upstream issue was inspected for it.
