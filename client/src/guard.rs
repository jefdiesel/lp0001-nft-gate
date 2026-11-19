// SPDX-License-Identifier: MIT OR Apache-2.0
//! The fail-closed outcome check's hook (spec §4.4 item 6).
//!
//! The stock wallet crate (LEZ v0.4.0-rc3) proves and sends in one call: `send_proven` runs
//! `lee::execute_and_prove` and then `send_transaction` with no step between
//! (`lez/wallet/src/lib.rs:1814-1900`); its only hook, `tx_pre_check`, sees the pre-states
//! (`:1649`). But `WalletCore` is generic over its sequencer client
//! (`WalletSequencerClient`, `:302-366`). So this client wraps the real one, and its
//! `send_transaction` runs the outcome check on the proven transaction itself and refuses to
//! forward it. The check therefore runs after proving and before sending: it never stops the
//! 10 minutes of proving, but nothing unexpected ever reaches the sequencer.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use common::{block::Block, transaction::LeeTransaction, HashType, PageLimit};
use lee::{Account, AccountId, Actor, EncryptedNote, ProgramId, Publication};
use lee_core::{account::Nonce, BlockId, Commitment, CommitmentPosition, CommitmentSetDigest, MembershipProof};
use sequencer_service_rpc::SequencerClient;
use wallet::{ExecutionFailureKind, WalletSequencerClient};

/// A check of the proven transaction; `Err` stops it.
pub type GuardFn = Box<dyn Fn(&LeeTransaction) -> Result<(), String> + Send + Sync>;

/// The real sequencer client, with an optional guard on `send_transaction`.
#[derive(Clone)]
pub struct GuardedClient {
    pub inner: SequencerClient,
    guard: Arc<Mutex<Option<GuardFn>>>,
    /// Every transaction the guard let through, by the order sent.
    pub passed: Arc<Mutex<Vec<String>>>,
}

impl GuardedClient {
    pub fn new(inner: SequencerClient) -> Self {
        Self { inner, guard: Arc::new(Mutex::new(None)), passed: Arc::new(Mutex::new(Vec::new())) }
    }

    pub fn set_guard(&self, guard: Option<GuardFn>) {
        *self.guard.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = guard;
    }
}

#[derive(Debug)]
pub enum GuardError {
    Client(sequencer_service_rpc::ClientError),
    Refused(String),
}

impl std::fmt::Display for GuardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Client(e) => write!(f, "{e}"),
            Self::Refused(why) => write!(f, "UNEXPECTED_OUTCOME: {why}"),
        }
    }
}

impl std::error::Error for GuardError {}

impl From<GuardError> for ExecutionFailureKind {
    fn from(e: GuardError) -> Self {
        match e {
            GuardError::Client(c) => c.into(),
            GuardError::Refused(why) => {
                Self::SequencerClientError(format!("UNEXPECTED_OUTCOME (not sent): {why}"))
            }
        }
    }
}

type R<T> = Result<T, GuardError>;

impl WalletSequencerClient for GuardedClient {
    type Error = GuardError;

    async fn send_transaction(&self, tx: LeeTransaction) -> R<HashType> {
        let verdict = {
            let guard = self.guard.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.as_ref().map(|check| check(&tx))
        };
        if let Some(Err(why)) = verdict {
            return Err(GuardError::Refused(why));
        }
        let hash = WalletSequencerClient::send_transaction(&self.inner, tx).await.map_err(GuardError::Client)?;
        self.passed.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(hash.to_string());
        Ok(hash)
    }

    async fn get_block(&self, block_id: BlockId) -> R<Option<Block>> {
        WalletSequencerClient::get_block(&self.inner, block_id).await.map_err(GuardError::Client)
    }

    async fn get_block_range(&self, start_block_id: BlockId, end_block_id: BlockId) -> R<Vec<Block>> {
        WalletSequencerClient::get_block_range(&self.inner, start_block_id, end_block_id).await.map_err(GuardError::Client)
    }

    async fn get_last_block_id(&self) -> R<BlockId> {
        WalletSequencerClient::get_last_block_id(&self.inner).await.map_err(GuardError::Client)
    }

    async fn get_account_balance(&self, account_id: AccountId) -> R<u128> {
        WalletSequencerClient::get_account_balance(&self.inner, account_id).await.map_err(GuardError::Client)
    }

    async fn get_transaction(&self, tx_hash: HashType) -> R<Option<(LeeTransaction, BlockId)>> {
        WalletSequencerClient::get_transaction(&self.inner, tx_hash).await.map_err(GuardError::Client)
    }

    async fn get_accounts_nonces(&self, account_ids: Vec<AccountId>) -> R<Vec<Nonce>> {
        WalletSequencerClient::get_accounts_nonces(&self.inner, account_ids).await.map_err(GuardError::Client)
    }

    async fn get_account_view(&self, actor_state_selector: Actor) -> R<Option<Account>> {
        WalletSequencerClient::get_account_view(&self.inner, actor_state_selector).await.map_err(GuardError::Client)
    }

    async fn get_proofs_and_root(
        &self,
        commitments: Vec<Commitment>,
        message_positions: Vec<CommitmentPosition>,
    ) -> R<(Vec<Option<MembershipProof>>, Vec<Option<MembershipProof>>, CommitmentSetDigest)> {
        WalletSequencerClient::get_proofs_and_root(&self.inner, commitments, message_positions)
            .await
            .map_err(GuardError::Client)
    }

    async fn get_fee_state(&self) -> R<sequencer_service_rpc::FeeStateQuote> {
        WalletSequencerClient::get_fee_state(&self.inner).await.map_err(GuardError::Client)
    }

    async fn get_account(&self, account_id: AccountId) -> R<Option<Account>> {
        WalletSequencerClient::get_account(&self.inner, account_id).await.map_err(GuardError::Client)
    }

    async fn get_program_ids(&self) -> R<BTreeMap<String, ProgramId>> {
        WalletSequencerClient::get_program_ids(&self.inner).await.map_err(GuardError::Client)
    }

    async fn get_publications(&self, from_position: CommitmentPosition, limit: PageLimit) -> R<Vec<(CommitmentPosition, Publication)>> {
        WalletSequencerClient::get_publications(&self.inner, from_position, limit).await.map_err(GuardError::Client)
    }

    async fn get_recovery_binding(&self, address: AccountId) -> R<Option<EncryptedNote>> {
        WalletSequencerClient::get_recovery_binding(&self.inner, address).await.map_err(GuardError::Client)
    }
}
