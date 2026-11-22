# Members' door (Basecamp mini-app)

Members' door lets you prove you hold an NFT of a collection without showing which NFT or
which account holds it. Names Wallet makes the proof and returns a pass id; this app never
sees a key, an account, or a balance. It is the LP-0001 reference app (testnet only).

| | |
|---|---|
| Basecamp name | `members_door_ui`, shown as "Members' door" |
| Author | `lns:names` |
| Type | `ui_qml` (`qml/Main.qml`, icon `src/icons/icon.svg`) |
| Dependency | `names >= 0.18.0` (called via `NamesWallet.qml`) |
| Capability | `membership-proof` only — no account access |
| Status | Packaged (`flake.nix`, `metadata.json`, version `0.1.0`), build/install untested here |

## Building and installing in Basecamp

`flake.nix` builds the `ui_qml` package with `logos-module-builder.lib.mkLogosQmlModule`
using `metadata.json`, declaring the `names` (`>=0.18.0`) dependency and `names.url = "path:../names"`
flake input (override with `--override-input names path:/abs/path/to/basecamp-module` when
building outside a sibling checkout).

Network defaults live in one per-network file (`qml/networks.mjs`, testnet only): no
program id is hard-coded in `qml/Main.qml` or `qml/logic.mjs`. An unknown wallet zone does
not fall back to `rc3` — the app states the network is not set up and disables both actions.
Collection and consumer program ids can be entered in the app for a session.

## The screen

The page is one left-aligned column next to Basecamp's menu, at most 680 px wide and 56 px
from the window edge (24 px when narrow), styled with the Names Wallet / Vest kit copied
into `qml/kit/`. One primary button ("Check in this app") is strong; secondary controls
are plain outline buttons or links. Every press gives immediate feedback and ends in a
plain result.

- **Check in this app** (primary, off-chain): sends `request_membership_proof` with
  `program` (the network entry's `nftProgram`), `collection`, `purpose` (at most 120 chars),
  `nonce`, and `ttl`, with no account, audience, or context field.
- **On-chain use** ("Sign the members' book"): keeps the on-chain flow in the app and
  maps Names 0.18.3's real on-chain refusal plainly while leaving unknown errors unknown.
- **Live status**: follows Names 0.18.3's `app_view()` contract (`status.state`:
  `awaiting-approval`, `checking`, `proving` with `elapsed_seconds`, `sending`,
  `waiting-for-block`, `ready`, `failed` with `code`/`plain_message`, `cancelled`, plus
  top-level `rejected` and `expired`). A failed status read keeps polling and never
  re-enables a second request.
- **Unchecked pass state**: this app has no chain reader yet, so a well-shaped `ready`
  pass is shown neutrally as "Proof sent. Not checked on chain by this app yet." — never
  green and never "Valid".
- **What a pass reveals**: states upfront and on the returned pass card what is revealed
  and what is not revealed.

## Tests

From `miniapp/`, run `node --test test/*.test.mjs`. This tests the web page logic,
`qml/logic.mjs` against `vectors/gate-vectors.json`, Names 0.18.3 `app_view()` fixtures,
all `WalletError` codes, unknown errors, cancel during proving, status read failure,
unknown zones, proof shape validation, privacy checks (no account-access calls), and
static Basecamp QML and packaging rules. QML rendering and `nix build` require Qt / Nix
on the build host.
