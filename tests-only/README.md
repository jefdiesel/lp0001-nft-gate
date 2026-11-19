# tests-only: never deployed outside the throwaway chain

`hostile/` is the attacker stand-in for test T4 (spec §8.1): an NFT-shaped program that, when
asked `ReadState` by the unsafe test gate, sends `Transfer` to the holder's real reference-NFT
actor and answers with a forged "owned copy". It holds no NFT state and is not an NFT program;
it exists to show (a) whether LEZ admits a program acting with the holder's authority and
(b) that the client's fail-closed outcome check stops the transaction before it is sent.

It is not in the logic workspace, is never in any deploy list but T4's, and is deployed only
on the throwaway chain at 127.0.0.1:3310.
