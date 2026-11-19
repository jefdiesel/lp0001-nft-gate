// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;

const GATE: Bytes32 = [0x6A; 32];
const NFT: Bytes32 = [0xAA; 32];
const INBOX: Bytes32 = [0xFE; 32];
const SECRET: Bytes32 = [0x5E; 32];
const NB: u64 = 1_763_500_200_000;

fn bal(v: u128) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn holder_state(pending: bool) -> Vec<u8> {
    let s = GateState::Holder {
        secret: SECRET,
        pending: pending.then_some(gate_core::Pending { collection: [1; 32], binding: [2; 32], not_before: NB, expires_at: NB + 1 }),
    };
    let mut buf = [0_u8; gate_core::HOLDER_LEN_PENDING];
    let n = s.encode_into(&mut buf);
    buf[..n].to_vec()
}

fn expected() -> Expected {
    Expected {
        gate: GATE,
        nft_program: NFT,
        slot: gate_core::slot::<Sha2>(&SECRET),
        fact: Fact { binding: [0xB1; 32], not_before: NB, expires_at: NB + 3_600_000 },
        secret: SECRET,
        fee_inbox: INBOX,
        max_fee: 6_000_000,
        holder_credit: 0,
        payer_credit: 0,
    }
}

fn good() -> (ObservedPublic, ObservedPrivate) {
    let e = expected();
    let pda = gate_core::slot_pda::<Sha2>(&GATE, &e.slot);
    let public = ObservedPublic {
        public_root: false,
        public_actors: vec![(pda, GATE), (INBOX, NATIVE_TOKEN_PROGRAM)],
        boundary: vec![
            Step::PrivateToPublic { to: (pda, GATE), message: publish_message(&e.slot, &e.fact) },
            Step::EndPublicSubtree,
            Step::PrivateToPublic { to: (INBOX, NATIVE_TOKEN_PROGRAM), message: vec![1, 2, 3] },
            Step::EndPublicSubtree,
        ],
        window: (Some(NB), Some(NB + 3_600_000)),
        async_messages: 0,
    };
    let mut before = States::new();
    before.insert(NFT, vec![2, 1, 1]);
    before.insert(NATIVE_TOKEN_PROGRAM, bal(20_000_000));
    before.insert([0x77; 32], vec![9]);
    let mut after = before.clone();
    after.insert(GATE, holder_state(false));
    after.insert(NATIVE_TOKEN_PROGRAM, bal(20_000_000 - 5_070_616));
    (public, ObservedPrivate { holder_before: before, holder_after: after, payer: None })
}

fn fails(public: &ObservedPublic, private: &ObservedPrivate, needle: &str) {
    fails_with(&expected(), public, private, needle);
}

fn fails_with(e: &Expected, public: &ObservedPublic, private: &ObservedPrivate, needle: &str) {
    let r = outcome_check(e, public, private);
    let errs = r.expect_err("must stop the proof");
    assert!(errs.iter().any(|e| e.contains(needle)), "{needle}: {errs:?}");
}

#[test]
fn a_correct_prove_passes() {
    let (p, q) = good();
    assert_eq!(outcome_check(&expected(), &p, &q), Ok(()));
}

#[test]
fn a_public_root_stops_it() {
    let (mut p, q) = good();
    p.public_root = true;
    fails(&p, &q, "root message would be public");
}

#[test]
fn another_public_actor_stops_it() {
    let (mut p, q) = good();
    p.public_actors.push(([0x33; 32], NFT));
    fails(&p, &q, "unexpected public actor");
}

#[test]
fn a_second_write_or_a_wrong_fact_stops_it() {
    let (mut p, q) = good();
    let e = expected();
    let pda = gate_core::slot_pda::<Sha2>(&GATE, &e.slot);
    p.boundary.push(Step::PrivateToPublic { to: (pda, GATE), message: publish_message(&e.slot, &e.fact) });
    fails(&p, &q, "2 slot writes");
    let (mut p, q) = good();
    p.boundary[0] = Step::PrivateToPublic { to: (pda, GATE), message: publish_message(&e.slot, &Fact { expires_at: NB + 86_400_000, ..e.fact }) };
    fails(&p, &q, "not the expected fact");
    let (mut p, q) = good();
    p.boundary.push(Step::PrivateToPublic { to: ([0x44; 32], NFT), message: vec![] });
    fails(&p, &q, "besides the slot");
    let (mut p, q) = good();
    p.boundary.push(Step::PublicToPrivate { to: ([0x44; 32], NFT), message: vec![] });
    fails(&p, &q, "into the private part");
    let (mut p, q) = good();
    p.boundary.remove(0);
    fails(&p, &q, "0 slot writes");
    let (mut p, q) = good();
    p.boundary.remove(2);
    fails(&p, &q, "0 fee credits");
}

#[test]
fn a_wrong_or_empty_window_stops_it() {
    let (mut p, q) = good();
    p.window = (None, None);
    fails(&p, &q, "window");
    let (mut p, q) = good();
    p.window = (Some(NB), Some(NB + 86_400_000));
    fails(&p, &q, "window");
}

#[test]
fn a_pending_async_message_stops_it() {
    let (mut p, q) = good();
    p.async_messages = 1;
    fails(&p, &q, "asynchronous");
}

#[test]
fn any_change_to_the_nft_actor_stops_it() {
    // T4's hostile Transfer shows up exactly here.
    let (p, mut q) = good();
    q.holder_after.insert(NFT, vec![2, 1, 0]);
    fails(&p, &q, "NFT actor changed");
    let (p, mut q) = good();
    q.holder_after.remove(&NFT);
    fails(&p, &q, "NFT actor changed");
}

#[test]
fn the_gate_state_must_be_an_idle_holder_with_the_same_secret() {
    let (p, mut q) = good();
    q.holder_after.insert(GATE, holder_state(true));
    fails(&p, &q, "gate state after");
    let (p, mut q) = good();
    q.holder_after.remove(&GATE);
    fails(&p, &q, "gate state after");
}

#[test]
fn another_actor_changing_stops_it() {
    let (p, mut q) = good();
    q.holder_after.insert([0x77; 32], vec![8]);
    fails(&p, &q, "changed");
    let (p, mut q) = good();
    q.holder_after.insert([0x78; 32], vec![1]);
    fails(&p, &q, "changed");
}

#[test]
fn the_balance_may_only_fall_by_the_fee() {
    let (p, mut q) = good();
    q.holder_after.insert(NATIVE_TOKEN_PROGRAM, bal(20_000_000 - 6_000_001));
    fails(&p, &q, "not a fee");
    let (p, mut q) = good();
    q.holder_after.insert(NATIVE_TOKEN_PROGRAM, bal(20_000_001));
    fails(&p, &q, "not a fee");
}

#[test]
fn a_separate_payer_pays_and_the_holder_does_not() {
    let (p, mut q) = good();
    q.holder_after.insert(NATIVE_TOKEN_PROGRAM, bal(20_000_000));
    let mut pb = States::new();
    pb.insert(NATIVE_TOKEN_PROGRAM, bal(9_000_000));
    let mut pa = pb.clone();
    pa.insert(NATIVE_TOKEN_PROGRAM, bal(9_000_000 - 5_070_616));
    q.payer = Some((pb.clone(), pa.clone()));
    assert_eq!(outcome_check(&expected(), &p, &q), Ok(()));
    let mut q2 = q.clone();
    q2.holder_after.insert(NATIVE_TOKEN_PROGRAM, bal(19_000_000));
    fails(&p, &q2, "holder's balance changed");
    let mut pa2 = pa;
    pa2.insert(NFT, vec![1]);
    q.payer = Some((pb, pa2));
    fails(&p, &q, "payer's actor");
}

#[test]
fn plan_follows_the_wallet_rule() {
    let now = NB + 123_456;
    let p = plan(now, 3_000_000).unwrap();
    assert_eq!(p, Plan { not_before: NB, ttl_class: 0, expires_at: NB + 3_600_000 });
    assert_eq!(plan(now, 7_200_000).unwrap().ttl_class, 1);
    assert_eq!(plan(now, 86_400_001), None);
}

#[test]
fn errors_classify() {
    assert_eq!(classify("Guest panicked: gate: NOT_A_MEMBER"), Some(WalletError::NotAMember));
    assert_eq!(classify("Transaction OutOfValidityWindow"), Some(WalletError::OutOfWindow));
    assert_eq!(classify("Nullifier already seen"), Some(WalletError::AccountBusy));
    assert_eq!(
        classify("Fee reserve of 5171104 from payer X fails, though the transaction runs without it"),
        Some(WalletError::FeeReserve)
    );
    assert_eq!(classify("UNEXPECTED_OUTCOME: the holder's NFT actor changed"), Some(WalletError::UnexpectedOutcome));
    assert_eq!(classify("something else"), None);
    for e in [
        WalletError::NotAMember,
        WalletError::FeeReserve,
        WalletError::AccountBusy,
        WalletError::OutOfWindow,
        WalletError::ProverOutOfMemory,
        WalletError::RpcUnreachable,
        WalletError::GateNotPinned,
        WalletError::UnexpectedOutcome,
    ] {
        assert!(!e.plain().is_empty() && e.code().chars().all(|c| c.is_ascii_uppercase() || c == '_'));
    }
}

#[test]
fn pre_checks() {
    assert!(enough_memory(16 * 1024 * 1024 * 1024));
    assert!(!enough_memory(8 * 1024 * 1024 * 1024));
    assert!(covers_reserve(OBSERVED_RESERVE, OBSERVED_RESERVE));
    assert!(!covers_reserve(OBSERVED_RESERVE - 1, OBSERVED_RESERVE));
    let img = [1_u32; 8];
    assert!(gate_is_pinned(&img, true, &img, &NFT, &NFT).is_ok());
    assert!(gate_is_pinned(&img, false, &img, &NFT, &NFT).is_err());
    assert!(gate_is_pinned(&[2; 8], true, &img, &NFT, &NFT).is_err());
    assert!(gate_is_pinned(&img, true, &img, &[0xAB; 32], &NFT).is_err());
}

#[test]
fn publish_bytes_layout() {
    let e = expected();
    let m = publish_message(&e.slot, &e.fact);
    assert_eq!(m.len(), 81);
    assert_eq!(m[0], 0);
    assert_eq!(&m[1..33], &e.slot);
}

#[test]
fn claimed_pending_credits_may_raise_a_balance_but_no_more() {
    // The stock wallet claims pending native credits (fee refunds) before the root.
    let e = Expected { holder_credit: 3_000_000, ..expected() };
    let (p, mut q) = good();
    q.holder_after.insert(NATIVE_TOKEN_PROGRAM, bal(20_000_000 + 3_000_000 - 5_070_616));
    assert_eq!(outcome_check(&e, &p, &q), Ok(()));
    // a refund larger than the fee: the balance rises, within the credits
    q.holder_after.insert(NATIVE_TOKEN_PROGRAM, bal(20_000_000 + 2_000_000));
    assert_eq!(outcome_check(&e, &p, &q), Ok(()));
    q.holder_after.insert(NATIVE_TOKEN_PROGRAM, bal(20_000_000 + 3_000_001));
    fails_with(&e, &p, &q, "not a fee");
    // without credits a rise is refused
    let (p, mut q) = good();
    q.holder_after.insert(NATIVE_TOKEN_PROGRAM, bal(20_000_000 + 1));
    fails(&p, &q, "not a fee");
}

#[test]
fn with_a_separate_payer_the_holder_may_only_gain_its_credits() {
    let e = Expected { holder_credit: 1_000, payer_credit: 0, ..expected() };
    let (p, mut q) = good();
    q.holder_after.insert(NATIVE_TOKEN_PROGRAM, bal(20_000_000 + 1_000));
    let mut pb = States::new();
    pb.insert(NATIVE_TOKEN_PROGRAM, bal(9_000_000));
    let mut pa = pb.clone();
    pa.insert(NATIVE_TOKEN_PROGRAM, bal(9_000_000 - 5_070_616));
    q.payer = Some((pb, pa));
    assert_eq!(outcome_check(&e, &p, &q), Ok(()));
    q.holder_after.insert(NATIVE_TOKEN_PROGRAM, bal(20_000_000 + 1_001));
    fails_with(&e, &p, &q, "holder's balance changed");
}
