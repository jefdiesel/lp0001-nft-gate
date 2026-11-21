# Members' door (Basecamp mini-app)

Members' door lets you prove you hold an NFT of a collection without showing which NFT or
which account holds it. Names Wallet makes the proof and gives back a pass id that an app can
use to check membership; this app never sees a key, an account or a balance. It is the LP-0001
reference app and, on this branch, a skeleton: not built or published.

| | |
|---|---|
| Basecamp name | `members_door_ui`, shown as "Members' door" |
| Package main | `members_door_ui_plugin`, a `ui_qml` app |
| Needs | the `names` module 0.18.0 or later; the Names app installed and open to answer the request |
| Capability | `membership-proof` only — no account access |
| Status | Skeleton (version 0.1.0), not built or published |

## Installing in Basecamp

There is no published package yet. When one is built, add it in Basecamp with Package Manager →
Install Local Package and pick the `members_door_ui` package. `metadata.json` declares one
dependency: the `names` module at version 0.18.0 or later. The package's main file is
`members_door_ui_plugin` and its view is `Main.qml`, drawn with the shared design kit.

The proof is answered in the Names app, so Names must be installed and open. The collection is
pinned into the app's configuration at build time; with no collection set, the screen says so
and asks for one to be added to the app's settings.

## The screen

The page is one left-aligned column, at most 680 px wide and 56 px from the window edge (24 px
when narrow). The header is `PageHeader` with the title "Members' door", the sentence "Prove you
hold an NFT of the collection without showing which NFT or which account.", and a network line of
Runs on Logos test network · Names Wallet. Only one button is strong at a time; the rest are
plain buttons or links. Every press changes a button's label at once and ends in a plain result
line. Red is used only for a real problem.

## How it works

The How it works card teaches these three steps, word for word from `doorSteps` in `logic.mjs`.
The current step is marked and its help is printed under it.

1. **Ask for a proof** — Names Wallet asks you to prove you hold an NFT of the collection. This app never sees your account or which NFT you use.
2. **Approve in the wallet** — The wallet shows what becomes public: a pass id and when it starts and ends. Proving takes about 10 minutes; you come back here after.
3. **Read your pass** — Your pass id is below. Show it to an app that asks for it; it says nothing about which NFT or account you used.

While the request waits, step 2's help reads "Names Wallet is open with the request: approve the
proof. You come back here after."

## Buttons and links

- "Check in this app" is the one strong button. It asks Names Wallet for a proof (step 1, or the
  small card shown when the steps are hidden). It reads "Preparing…" the moment it is pressed and
  "Waiting for you in Names…" while the request waits. It is disabled while the wallet is busy or
  a request is pending.
- "Check the status" is a link on step 2; it reads the request status again at once. The page
  also reads it every five seconds while it is open and in front.
- "Got it" folds the steps away once the pass is ready. "Steps" brings them back and "Hide steps"
  hides them again.
- "Sign the members' book" is shown but disabled: on-chain use needs a pinned gate, which this
  wallet does not have yet. It makes no request.
- The plain line at the bottom reports the last step. "Before you start" holds the two red
  warnings when the wallet bridge is missing or no collection is configured. "Technical details"
  holds the full collection id and a note on what the proof does.

## What it asks Names Wallet for

Two calls to the `names` module do all the work:

- `request_membership_proof` sends the configured collection, the purpose "Check in this app", a
  fresh random nonce and a one-hour lifetime (`3600000` ms). No account, audience or on-chain
  target is sent.
- `membership_proof_status` sends the request id. It is read every five seconds while the app is
  open and in front, and stops when the proof is ready or the request fails, is declined, expires
  or is cancelled.

A busy reply is retried up to six times, 500 ms apart; if the wallet is still busy, the app says
to try again in a moment. There is no account-access call: the app asks for no account, balance,
holding or LEZ payment, and holds no key. The wallet returns only a pass id and when it is valid;
which NFT and which account were used stay private. The proof itself runs in the Names app and
takes about 10 minutes.

## Tests

`node --test miniapp/test/*.test.mjs` from the repository root covers the page logic, the
QML-safe logic against the shared vectors, and the plain-text helpers. The QML layout is not
unit-tested; it needs a Qt runner. See the [mini-app README](../README.md) for both forms of this
app and the [wallet integration notes](../../docs/WALLET-INTEGRATION.md) for the module interface.
