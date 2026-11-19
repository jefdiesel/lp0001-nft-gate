# Members' door (working name): the token-gating mini-app

Spec §5. Two forms, neither deployed:

- `web/`: a static page (HTML + ES modules, no server, no Worker). Serve the repo root with any
  static server and open `miniapp/web/`. In Basecamp it talks to the wallet through
  `js/wallet-shim`; in a plain browser there is no wallet, so screen 2 verifies a pasted proof
  object from `gate-cli prove --out` against the RPC you name, and screen 3 shows a gated action
  from its `Act` transaction hash. The RPC must allow the page's origin (a local sequencer may
  need a CORS-allowing proxy of your own).
- `basecamp/`: a Basecamp `ui_qml` skeleton (metadata, `Main.qml`, QML-safe `logic.mjs`). It
  calls only the `membership-proof` methods (docs/WALLET-INTEGRATION.md), which the Names
  Wallet does not have yet, and needs a C++ backend for chain reads (as the Testimonial app has).

Screens: (1) request: "Check in this app" / "Sign the members' book on chain"; (2) off-chain
use: "Valid" or the code and its plain meaning, "Establishes: holder of [collection], until
[time]", "Does not reveal: which NFT, which account", and the RPC named as trusted; (3) on-chain
use: the gated action (acting account, block, pass) and the same pass checked in the page's own
code ("same proof, both ways"). Failure demos come from the wallet's codes and the verifier's
(expired, superseded = BINDING_MISMATCH, wrong collection, NOT_A_MEMBER).

Tests: `node --test test/*.test.mjs` (page logic; the QML-safe logic against the shared vectors).
