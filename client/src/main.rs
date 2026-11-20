// SPDX-License-Identifier: MIT OR Apache-2.0
//! `gate-cli`: the LP-0001 client on the stock LEZ v0.4.0-rc3 wallet crate.
//!
//! - `prove`: the pre-checks of spec §4.4, then one private `Prove` rooted on the holder, with
//!   an explicit fee payer, and the fail-closed outcome check on the proven transaction before
//!   it is sent (see `guard.rs`).
//! - `act`: the on-chain use, a public `Act` to the reference consumer.
//! - `read-slot`, `verify`, `gated`: read a slot, check it in app code, show a gated action.
//! - `nft ...`, `wallet ...`: setup on the throwaway chain (the reference NFT program's own
//!   messages, and the LEZ wallet's command line).
//! - Test-only switches, each named `--test-*` and printed loudly, for T3, T4 and T6.
//!
//! Safety: loopback sequencers only, as nft-drive. The wallet's default sequencer is the public
//! testnet; this tool always overrides it and refuses any address that is not loopback.

mod guard;

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Context as _, Result};
use clap::{Args, Parser, Subcommand};
use common::{transaction::LeeTransaction, HashType};
use gate_core::{Fact, GateState, Outcome, Sha2};
use gate_interface::{ConsumerMessage, ConsumerState, GateMessage};
use gate_wallet_rules::{ObservedPrivate, ObservedPublic, States, Step, WalletError};
use guard::{GuardFn, GuardedClient};
use lee::privacy_preserving_transaction::circuit::ProgramCatalog;
use lee_core::{
    account::{Account, AccountId, Actor},
    execution_state::BoundaryStep,
    program::{PdaSeed, ProgramHeader, PROGRAM_LOADER_ACCOUNT_ID},
};
use nft_core::{Message as NftMessage, NftState};
use wallet::{
    account::{AccountIdWithPrivacy, Label},
    config::{SequencerConnectionData, WalletConfigOverrides},
    utils::{fetch_config_path, fetch_persistent_storage_path, read_password},
    AccountIdentity, AccountMention, AsyncDelivery, DryRunReport, ExecutionFailureKind, WalletCore,
};

type Wallet = WalletCore<GuardedClient>;

#[derive(Parser)]
#[command(name = "gate-cli", about = "LP-0001 private NFT membership proof: prove, act, verify")]
struct Cli {
    /// The sequencer. Loopback only.
    #[arg(long, env = "GATE_CLI_SEQUENCER", default_value = "http://127.0.0.1:3310")]
    sequencer: String,
    /// The deployment config (gate, NFT program, image id, consumer, zone, submission id).
    #[arg(long, env = "GATE_CONFIG")]
    config: Option<PathBuf>,
    /// Optional LEZ indexer RPC for getEvents (defaults to the sequencer RPC).
    #[arg(long)]
    events_url: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// The LEZ wallet's own command line (accounts, transfers, pending, program-loader).
    Wallet {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// The reference NFT program's messages, for setting up holdings on the throwaway chain.
    Nft {
        /// The reference NFT program (no deployment config needed for setup).
        #[arg(long, env = "GATE_NFT_PROGRAM")]
        program: String,
        #[command(subcommand)]
        cmd: NftCommand,
    },
    /// Print the zone's channel id (hex), for the deploy constants.
    ChannelId,
    /// Print a program's header: image id (hex), immutable flag, first segment.
    Header { #[arg(long)] program: String },
    /// Make a membership proof: one private Prove rooted on the holder.
    Prove(ProveArgs),
    /// The on-chain use: a public Act to the reference consumer, signed by `--actor`.
    Act {
        #[arg(long)]
        actor: String,
        /// The slot, 64 hex.
        #[arg(long)]
        slot: String,
        /// A public account paying the fee (default: the actor).
        #[arg(long)]
        payer: Option<String>,
    },
    /// Print the holder's slot and slot account (from the wallet's own note).
    Slot {
        #[arg(long)]
        holder: String,
        /// TEST ONLY (T1): also print the slot secret, to search transactions for it.
        #[arg(long)]
        test_show_secret: bool,
    },
    /// Read a slot's gate state from the chain and decode it.
    ReadSlot {
        #[arg(long)]
        slot: String,
        /// The gate (default: the config's).
        #[arg(long)]
        gate: Option<String>,
    },
    /// Check a slot in app code (no transaction), for an off-chain or on-chain context.
    Verify(VerifyArgs),
    /// Show a gated action from its Act transaction: actor, slot, block time, reverted or not.
    Gated { #[arg(long)] tx: String },
    /// Decode an included transaction field by field (from nft-drive-v2).
    DecodeTx { #[arg(long)] hash: String },
    /// Each private account of this wallet: id, commitment, next nullifier (from nft-drive-v2).
    Snapshot,
}

#[derive(Subcommand)]
enum NftCommand {
    /// Create a collection: definition and master, both public accounts of this wallet.
    NewDefinition {
        #[arg(long)] definition: String,
        #[arg(long)] master: String,
        #[arg(long)] name: String,
        #[arg(long)] supply: u128,
        #[arg(long)] payer: Option<String>,
    },
    /// Open a holding of `--definition` on `--from` (public or private).
    InitHolding { #[arg(long)] definition: String, #[arg(long)] from: String, #[arg(long)] payer: Option<String> },
    /// Print one copy from the master `--from` to `--to`.
    Print { #[arg(long)] from: String, #[arg(long)] to: String, #[arg(long)] payer: Option<String> },
    /// Move the whole value at `--from` to `--to`.
    Transfer { #[arg(long)] from: String, #[arg(long)] to: String, #[arg(long)] payer: Option<String> },
    /// Print `--account`'s NFT state.
    Read { #[arg(long)] account: String },
    /// Receive this wallet's pending NFT credits (private holdings).
    Pending { #[arg(long)] payer: Option<String> },
}

#[derive(Args)]
struct ProveArgs {
    /// The private account holding the NFT (label or id).
    #[arg(long)]
    holder: String,
    /// The private account paying the fee. Always explicit (spec §4.3). OWNER DECISION Q5,
    /// default: the holding pays for itself; a separate gas account only once T5 passes.
    #[arg(long)]
    payer: String,
    /// The collection (NFT definition id). Default: the config's.
    #[arg(long)]
    collection: Option<String>,
    /// The validity wanted, in ms; the wallet picks the smallest TTL class covering it.
    #[arg(long, default_value_t = 3_600_000)]
    ttl_ms: u64,
    // ---- off-chain context ----
    /// Off-chain: the caller identity the wallet's provider sees (the audience).
    #[arg(long, conflicts_with = "onchain_actor")]
    caller_identity: Option<String>,
    /// Off-chain: the app's purpose string.
    #[arg(long, requires = "caller_identity")]
    purpose: Option<String>,
    /// Off-chain: the app's 32-byte nonce, hex.
    #[arg(long, requires = "caller_identity")]
    app_nonce: Option<String>,
    // ---- on-chain context ----
    /// On-chain: the public account that will send `Act` (its consumer nonce is read here).
    /// OWNER DECISION Q3, default: a public account the user picks inside the wallet.
    #[arg(long)]
    onchain_actor: Option<String>,
    /// Write the proof object (JSON) here.
    #[arg(long)]
    out: Option<PathBuf>,
    // ---- TEST ONLY ----
    /// TEST ONLY (T6): use this not_before (ms) instead of the wallet rule.
    #[arg(long, requires = "test_ttl_class")]
    test_not_before: Option<u64>,
    /// TEST ONLY (T6): use this ttl_class (0..2 valid) instead of the wallet rule.
    #[arg(long, requires = "test_not_before")]
    test_ttl_class: Option<u8>,
    /// TEST ONLY (T3): mention the holder `.receiving()` twice, so its witness holds only `nsk`.
    #[arg(long)]
    test_unauthorized_root: bool,
    /// TEST ONLY (T3 control, T4): send to the config's `unsafe_test_gate`.
    #[arg(long)]
    test_unsafe_gate: bool,
    /// TEST ONLY (T4): the NFT program the unsafe test gate is told to read.
    #[arg(long, requires = "test_unsafe_gate")]
    test_nft_program: Option<String>,
    /// TEST ONLY (T4): also declare this account's actor under the real NFT program.
    #[arg(long)]
    test_declare: Vec<String>,
    /// TEST ONLY: do not check this wallet's own note for membership first (T2 then shows the
    /// gate's own refusal in simulation).
    #[arg(long)]
    test_skip_local_member_check: bool,
    /// TEST ONLY: turn the fail-closed outcome check off (diagnosis only; prints loudly).
    #[arg(long)]
    test_skip_outcome_check: bool,
}

#[derive(Args)]
struct VerifyArgs {
    #[arg(long)]
    slot: String,
    #[arg(long)]
    collection: Option<String>,
    #[arg(long, conflicts_with = "actor")]
    caller_identity: Option<String>,
    #[arg(long, requires = "caller_identity")]
    purpose: Option<String>,
    #[arg(long, requires = "caller_identity")]
    app_nonce: Option<String>,
    /// On-chain re-check: the hash of the Act transaction whose Gated event is read.
    #[arg(long, conflicts_with = "caller_identity")]
    tx_hash: Option<String>,
}

/// The deployment config, written by tests/onchain/01-deploy.sh.
#[derive(Debug, Clone)]
struct GateConfig {
    gate: AccountId,
    gate_image_id: Option<[u8; 32]>,
    nft_program: AccountId,
    consumer: Option<AccountId>,
    zone_id: [u8; 32],
    submission_id: [u8; 32],
    collection: Option<AccountId>,
    unsafe_test_gate: Option<AccountId>,
    max_fee: u128,
}

fn load_config(path: Option<&PathBuf>) -> Result<GateConfig> {
    let path = path.ok_or_else(|| anyhow!("give --config (or GATE_CONFIG): the deployment config"))?;
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).with_context(|| format!("read {}", path.display()))?)?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).filter(|x| !x.is_empty());
    let id = |k: &str| -> Result<Option<AccountId>> { s(k).map(|t| parse_id(t)?.ok_or_else(|| anyhow!("{k}: not an id"))).transpose() };
    let h32 = |k: &str| -> Result<Option<[u8; 32]>> { s(k).map(|t| gate_verify::unhex32(t).map_err(|e| anyhow!("{k}: {e}"))).transpose() };
    Ok(GateConfig {
        gate: id("gate")?.ok_or_else(|| anyhow!("config: gate missing"))?,
        gate_image_id: h32("gate_image_id")?,
        nft_program: id("nft_program")?.ok_or_else(|| anyhow!("config: nft_program missing"))?,
        consumer: id("consumer")?,
        zone_id: h32("zone_id")?.ok_or_else(|| anyhow!("config: zone_id missing"))?,
        submission_id: h32("submission_id")?.unwrap_or([0; 32]),
        collection: id("collection")?,
        unsafe_test_gate: id("unsafe_test_gate")?,
        max_fee: s("max_fee").map(str::parse).transpose()?.unwrap_or(6_000_000),
    })
}

#[tokio::main]
async fn main() {
    if let Err(e) = real_main().await {
        let text = format!("{e:#}");
        match gate_wallet_rules::classify(&text) {
            Some(code) => {
                eprintln!("FAILED {}: {}", code.code(), code.plain());
                eprintln!("detail: {text}");
            }
            None => eprintln!("FAILED: {text}"),
        }
        std::process::exit(1);
    }
}

async fn real_main() -> Result<()> {
    let cli = Cli::parse();
    let url = loopback(&cli.sequencer)?;
    let overrides = WalletConfigOverrides {
        sequencer: Some(SequencerConnectionData { sequencer_addr: url.clone(), basic_auth: None }),
        ..WalletConfigOverrides::default()
    };
    if let Command::Wallet { args } = cli.command {
        return run_wallet(args, overrides).await;
    }
    let client = GuardedClient::new(sequencer_service_rpc::SequencerClientBuilder::default().build(url.as_str())?);
    let storage_path = fetch_persistent_storage_path()?;
    if !storage_path.exists() {
        bail!("no wallet yet: set LEE_WALLET_HOME_DIR, or run `gate-cli wallet ...` to create one");
    }
    let password = read_password(&storage_path)?;
    let mut wallet: Wallet =
        WalletCore::new_update_chain_with_client(fetch_config_path()?, storage_path, Some(overrides), &password, client.clone())?;
    match cli.command {
        Command::Wallet { .. } => unreachable!(),
        Command::ChannelId => {
            use sequencer_service_rpc::RpcClient as _;
            let id = client.inner.get_channel_id().await?;
            println!("{id}");
            Ok(())
        }
        Command::Header { program } => {
            let program = resolve(&wallet, &program)?;
            let (image, immutable, first) = read_header(&wallet, program).await?;
            println!("program {program}\nimage_id {}\nimmutable {immutable}\nfirst_segment {first}", gate_verify::hex(&image_bytes(&image)));
            Ok(())
        }
        Command::Nft { program, cmd } => {
            let program = resolve(&wallet, &program)?;
            nft(&mut wallet, program, cmd).await
        }
        Command::Prove(args) => {
            let cfg = load_config(cli.config.as_ref())?;
            prove(&mut wallet, &client, &cfg, args).await
        }
        Command::Act { actor, slot, payer } => {
            let cfg = load_config(cli.config.as_ref())?;
            act(&mut wallet, &cfg, &actor, &slot, payer.as_deref()).await
        }
        Command::Slot { holder, test_show_secret } => {
            let cfg = load_config(cli.config.as_ref())?;
            let holder = resolve(&wallet, &holder)?;
            wallet.sync_to_latest_block().await?;
            let account = wallet.get_account_private(holder).ok_or_else(|| anyhow!("{holder} is not a private account of this wallet"))?;
            match GateState::decode(account.data.actor_state(cfg.gate).as_ref()) {
                Ok(GateState::Holder { secret, .. }) => {
                    let slot = gate_core::slot::<Sha2>(&secret);
                    println!("slot {}\nslot_account {}", gate_verify::hex(&slot), AccountId::new(gate_core::slot_pda::<Sha2>(cfg.gate.value(), &slot)));
                    if test_show_secret {
                        println!("TEST ONLY secret {}", gate_verify::hex(&secret));
                    }
                }
                other => println!("no slot yet ({other:?})"),
            }
            Ok(())
        }
        Command::ReadSlot { slot, gate } => {
            let cfg = load_config(cli.config.as_ref())?;
            let gate = match gate { Some(g) => resolve(&wallet, &g)?, None => cfg.gate };
            let slot = gate_verify::unhex32(&slot).map_err(|e| anyhow!(e))?;
            let pda = AccountId::new(gate_core::slot_pda::<Sha2>(gate.value(), &slot));
            let state = public_actor_state(&wallet, pda, gate).await?;
            println!("slot_account {pda}\nstate {} bytes {}", state.len(), gate_verify::hex(&state));
            match Fact::decode(&state) {
                Ok(f) => println!("fact binding {} not_before {} expires_at {}", gate_verify::hex(&f.binding), f.not_before, f.expires_at),
                Err(e) => println!("not a fact: {e:?}"),
            }
            Ok(())
        }
        Command::Verify(v) => {
            let cfg = load_config(cli.config.as_ref())?;
            verify(&mut wallet, &cfg, v, &url, &client, cli.events_url.as_deref()).await
        }
        Command::Gated { tx } => gated(&wallet, &tx).await,
        Command::DecodeTx { hash } => decode_tx(&wallet, &hash).await,
        Command::Snapshot => snapshot(&mut wallet).await,
    }
}

fn loopback(sequencer: &str) -> Result<url::Url> {
    let url: url::Url = sequencer.parse().context("--sequencer is not a URL")?;
    let host = url.host_str().unwrap_or_default();
    if !matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1") {
        bail!("{sequencer} is not loopback; this tool only talks to a local chain");
    }
    Ok(url)
}

// ---------------------------------------------------------------------------------------------
// prove
// ---------------------------------------------------------------------------------------------

async fn prove(wallet: &mut Wallet, client: &GuardedClient, cfg: &GateConfig, a: ProveArgs) -> Result<()> {
    let test = a.test_unsafe_gate || a.test_unauthorized_root || a.test_not_before.is_some() || !a.test_declare.is_empty()
        || a.test_skip_outcome_check || a.test_skip_local_member_check;
    if test {
        println!("!!! TEST ONLY switches are on: this is not a normal proof !!!");
    }
    let gate = if a.test_unsafe_gate {
        cfg.unsafe_test_gate.ok_or_else(|| anyhow!("config: unsafe_test_gate missing"))?
    } else {
        cfg.gate
    };
    let nft_program = match &a.test_nft_program { Some(p) => resolve(wallet, p)?, None => cfg.nft_program };
    let holder = resolve(wallet, &a.holder)?;
    let payer = resolve(wallet, &a.payer)?;
    let holder_identity = wallet
        .resolve_private_account(holder)
        .ok_or_else(|| anyhow!("{holder} is not a private account of this wallet: private holdings only (spec §4.6)"))?;
    wallet
        .resolve_private_account(payer)
        .ok_or_else(|| anyhow!("{payer}: the fee payer must be a private account of this wallet"))?;

    // Pre-check 1: memory.
    if std::env::var("RISC0_DEV_MODE").as_deref() == Ok("1") {
        println!("check 1 memory: skipped (RISC0_DEV_MODE=1, development receipts)");
    } else {
        let total = total_memory();
        if !gate_wallet_rules::enough_memory(total) {
            bail!("{}: {} GiB here", WalletError::ProverOutOfMemory.code(), total >> 30);
        }
        println!("check 1 memory: {} GiB, ok", total >> 30);
    }
    // Pre-check 4: the gate on chain is the pinned immutable image.
    if a.test_unsafe_gate {
        println!("check 4 pinned gate: SKIPPED (TEST ONLY unsafe gate {gate})");
    } else {
        let pinned = cfg.gate_image_id.ok_or_else(|| anyhow!("config: gate_image_id missing; cannot check the gate"))?;
        let (image, immutable, _) = read_header(wallet, gate).await.context("RPC_UNREACHABLE: reading the gate header")?;
        gate_wallet_rules::gate_is_pinned(&image, immutable, &words(&pinned), nft_program.value(), cfg.nft_program.value())
            .map_err(|e| anyhow!("GATE_NOT_PINNED: {e}"))?;
        println!("check 4 pinned gate: image {} immutable, NFT program {}", gate_verify::hex(&pinned), cfg.nft_program);
    }
    // Pre-check 3: the account is not busy with another of our sends.
    let _lock = HolderLock::take(holder)?;
    println!("check 3 account not busy: locked {holder} until inclusion");

    wallet.sync_to_latest_block().await?;
    let holder_account = wallet.get_account_private(holder).unwrap_or_default();
    let collection = match &a.collection {
        Some(c) => resolve(wallet, c)?,
        None => cfg.collection.ok_or_else(|| anyhow!("give --collection"))?,
    };
    if a.test_skip_local_member_check {
        println!("own-note membership: SKIPPED (TEST ONLY)");
    } else {
        let nft_state = holder_account.data.actor_state(cfg.nft_program);
        let member = match NftState::decode_opt(nft_state.as_ref()) {
            Some(NftState::Copy(c)) => c.definition_id == collection && c.owned,
            Some(NftState::Master(m)) => gate_core::MASTER_COUNTS && m.definition_id == collection && m.print_balance >= 1,
            _ => false,
        };
        if !member {
            bail!("NOT_A_MEMBER: the wallet's own note of {holder} holds no NFT of {collection}; nothing was sent");
        }
    }
    let (secret, slot_secret) = match GateState::decode(holder_account.data.actor_state(gate).as_ref()) {
        Err(gate_core::DecodeError::Empty) => {
            let s = random32()?;
            (s, Some(s))
        }
        Ok(GateState::Holder { secret, pending: None }) => (secret, None),
        other => bail!("the holder's gate state is {other:?}; refusing"),
    };
    let slot = gate_core::slot::<Sha2>(&secret);
    let pda = AccountId::new(gate_core::slot_pda::<Sha2>(gate.value(), &slot));

    // The window: the wallet rule on the sequencer's clock, or a TEST ONLY override.
    let chain_now = latest_timestamp(wallet).await?;
    let (not_before, ttl_class) = match (a.test_not_before, a.test_ttl_class) {
        (Some(nb), Some(c)) => {
            println!("window: TEST ONLY not_before {nb} ttl_class {c} (chain time {chain_now})");
            (nb, c)
        }
        _ => {
            let p = gate_wallet_rules::plan(chain_now, a.ttl_ms).ok_or_else(|| anyhow!("--ttl-ms is above the largest TTL class"))?;
            (p.not_before, p.ttl_class)
        }
    };
    let expires_at = not_before.saturating_add(*gate_core::TTL_MS.get(usize::from(ttl_class)).unwrap_or(&0));

    // The context, built here from the request's own values.
    let (context, context_desc) = match (&a.caller_identity, &a.onchain_actor) {
        (Some(id), None) => {
            let purpose = a.purpose.clone().unwrap_or_default();
            let nonce = match &a.app_nonce { Some(n) => gate_verify::unhex32(n).map_err(|e| anyhow!(e))?, None => bail!("give --app-nonce") };
            (
                gate_core::context_offchain::<Sha2>(&cfg.zone_id, gate.value(), id.as_bytes(), purpose.as_bytes(), &nonce),
                format!("off-chain caller {id:?} purpose {purpose:?} nonce {}", gate_verify::hex(&nonce)),
            )
        }
        (None, Some(actor)) => {
            let consumer = cfg.consumer.ok_or_else(|| anyhow!("config: consumer missing"))?;
            let actor = resolve(wallet, actor)?;
            let nonce = consumer_nonce(wallet, actor, consumer).await?;
            (
                gate_core::context_onchain::<Sha2>(&cfg.zone_id, gate.value(), consumer.value(), &cfg.submission_id, actor.value(), nonce),
                format!("on-chain consumer {consumer} actor {actor} nonce {nonce}"),
            )
        }
        _ => bail!("give either --caller-identity (off-chain) or --onchain-actor (on-chain)"),
    };
    let binding = gate_core::binding::<Sha2>(nft_program.value(), collection.value(), &context);
    println!("request: {context_desc}");
    println!("window [{not_before}, {expires_at}) ttl_class {ttl_class}; slot {} at {pda}", gate_verify::hex(&slot));

    let message = if a.test_unsafe_gate {
        GateMessage::TestProve { nft_program, collection, context, not_before, ttl_class, slot_secret }
    } else {
        GateMessage::Prove { collection, context, not_before, ttl_class, slot_secret }
    };
    let bytes = borsh::to_vec(&message)?;
    let mut extra = Vec::new();
    for d in &a.test_declare {
        extra.push(resolve(wallet, d)?);
    }
    let build_accounts = |w: &Wallet| -> Vec<AccountMention> {
        let mut g = holder_identity.clone().select_program_actor_state(gate);
        let mut n = holder_identity.clone().select_program_actor_state(nft_program);
        if a.test_unauthorized_root {
            g = g.receiving();
            n = n.receiving();
        }
        let mut v = vec![
            g,
            n,
            AccountIdentity::PublicPda { program: gate, seed: PdaSeed::new(slot) }.select_program_actor_state(gate).receiving(),
        ];
        if nft_program != cfg.nft_program {
            // T4: the hostile program writes to the holder's real NFT actor.
            v.push(holder_identity.clone().select_program_actor_state(cfg.nft_program));
        }
        for x in &extra {
            v.push(mention(w, *x).select_program_actor_state(cfg.nft_program).receiving());
        }
        v
    };
    let mut programs = vec![gate, nft_program];
    if nft_program != cfg.nft_program {
        programs.push(cfg.nft_program);
    }
    let catalog = catalog(wallet, &programs).await?;

    // Pre-check 5: simulate (the wallet's dry run).
    wallet.set_dry_run(true);
    let sim = wallet
        .send_tx_with_pre_check(build_accounts(wallet), 0, bytes.clone(), &catalog, None, AsyncDelivery::default(), Some(payer), |_| Ok(()))
        .await;
    wallet.set_dry_run(false);
    let reserve = match sim {
        Err(ExecutionFailureKind::DryRun(report)) => match *report {
            DryRunReport::Private { fee, headroom_reserve, .. } => {
                println!("check 5 simulation: passed; reserve {} (headroom {headroom_reserve}), payer {}", fee.reserve, fee.payer);
                headroom_reserve.max(fee.reserve)
            }
            DryRunReport::Public { .. } => bail!("the simulation priced a public transaction: the holder is not private"),
        },
        Err(e) => {
            let t = e.to_string();
            if t.contains("NOT_A_MEMBER") {
                bail!("NOT_A_MEMBER: refused in simulation: {t}");
            }
            bail!("refused in simulation: {t}");
        }
        Ok(_) => bail!("the dry run sent a transaction"),
    };
    // Pre-check 2: the payer covers the reserve alone.
    let payer_balance = wallet.get_account_private(payer).and_then(|acc| acc.data.native_balance().ok()).unwrap_or(0);
    if !gate_wallet_rules::covers_reserve(payer_balance, reserve) {
        bail!("FEE_RESERVE: payer {payer} holds {payer_balance}, the reserve is {reserve}");
    }
    println!("check 2 fee reserve: payer holds {payer_balance} >= {reserve}");

    // Pre-check 6: the fail-closed outcome check, on the proven transaction before it is sent.
    if a.test_skip_outcome_check {
        println!("check 6 outcome: OFF (TEST ONLY)");
    } else {
        let mut expected = gate_wallet_rules::Expected {
            gate: *gate.value(),
            nft_program: *cfg.nft_program.value(),
            slot,
            fact: Fact { binding, not_before, expires_at },
            secret,
            fee_inbox: *system_accounts::fee_inbox_account_id().value(),
            max_fee: cfg.max_fee.max(reserve),
            holder_credit: 0,
            payer_credit: 0,
        };
        // Pending native credits the wallet may claim before the root (refunds of earlier fees).
        let pending = wallet.owned_pending_messages().await.map_err(|e| anyhow!("RPC_UNREACHABLE: pending messages: {e}"))?;
        let credit_to = |id: AccountId| -> u128 {
            pending.iter().filter(|m| m.recipient.account_id() == id).filter_map(|m| m.native_credit()).sum()
        };
        let (holder_credit, payer_credit) = (credit_to(holder), if payer == holder { 0 } else { credit_to(payer) });
        expected.holder_credit = holder_credit;
        expected.payer_credit = payer_credit;
        println!("check 6 outcome: claimable credits holder {holder_credit} payer {payer_credit}");
        let key_chain = wallet.storage().key_chain().clone();
        let holder_before = states_of(&holder_account);
        let payer_before = (payer != holder).then(|| states_of(&wallet.get_account_private(payer).unwrap_or_default()));
        let check: GuardFn = Box::new(move |tx: &LeeTransaction| {
            let LeeTransaction::PrivacyPreserving(ptx) = tx else {
                return Err("not a privacy-preserving transaction".to_owned());
            };
            let m = &ptx.message;
            let e = &m.execution;
            let public = ObservedPublic {
                public_root: e.public_root.is_some(),
                public_actors: m.public_context.actors.iter().map(|x| (*x.account_id.value(), *x.program_account_id.value())).collect(),
                boundary: e.boundary.iter().map(step).collect(),
                window: (e.validity.timestamps.start(), e.validity.timestamps.end()),
                async_messages: e.async_messages.len(),
            };
            let mut kc = key_chain.clone();
            let mut index = kc.build_latest_nullifier_index();
            kc.follow_transaction(m, false, false, &mut index);
            let holder_after = kc.private_account(holder).map(|f| states_of(f.account)).ok_or("the holder is not in the key chain")?;
            let payer = payer_before
                .as_ref()
                .map(|before| (before.clone(), kc.private_account(payer).map(|f| states_of(f.account)).unwrap_or_default()));
            gate_wallet_rules::outcome_check(&expected, &public, &ObservedPrivate { holder_before: holder_before.clone(), holder_after, payer })
                .map_err(|v| v.join("; "))
        });
        client.set_guard(Some(check));
        println!("check 6 outcome: armed (runs on the proven transaction before it is sent)");
    }

    println!("proving on this computer...");
    let started = std::time::Instant::now();
    let nft_before = holder_account.data.actor_state(cfg.nft_program).to_vec();
    let sent = wallet
        .send_tx_with_pre_check(build_accounts(wallet), 0, bytes, &catalog, None, AsyncDelivery::default(), Some(payer), |pre| {
            // The NFT actor proven over is the one checked above.
            for row in pre {
                if row.selector == Actor::new(holder, cfg.nft_program) && row.data.as_ref() != nft_before.as_slice() {
                    return Err(ExecutionFailureKind::AccountDataError(holder));
                }
            }
            Ok(())
        })
        .await;
    client.set_guard(None);
    let (hash, _) = sent.map_err(|e| anyhow!("{e}"))?;
    println!("check 6 outcome: passed; proved and submitted in {:.0?}", started.elapsed());
    let block = finish(wallet, hash).await?;

    // Read back what a verifier reads.
    let state = public_actor_state(wallet, pda, gate).await?;
    let now = latest_timestamp(wallet).await?;
    let outcome = gate_core::check(&state, &binding, Some(now));
    println!("read back: {} ({})", outcome.name(), outcome.plain());
    let proof = serde_json::json!({
        "gate": gate.to_string(),
        "slot": gate_verify::hex(&slot),
        "slot_account": pda.to_string(),
        "not_before": not_before,
        "expires_at": expires_at,
        "tx_hash": hash.to_string(),
        "block": block,
    });
    println!("proof {proof}");
    if let Some(path) = a.out {
        std::fs::write(&path, serde_json::to_vec_pretty(&proof)?)?;
    }
    if outcome != Outcome::Ok {
        bail!("the fact read back is {}", outcome.name());
    }
    Ok(())
}

fn step(s: &BoundaryStep) -> Step {
    let actor = |a: &Actor| (*a.account_id.value(), *a.program_account_id.value());
    match s {
        BoundaryStep::PrivateToPublic(d) => Step::PrivateToPublic { to: actor(&d.envelope.to), message: d.envelope.message.clone() },
        BoundaryStep::PublicToPrivate(d) => Step::PublicToPrivate { to: actor(&d.envelope.to), message: d.envelope.message.clone() },
        BoundaryStep::EndPrivateSubtree => Step::EndPrivateSubtree,
        BoundaryStep::EndPublicSubtree => Step::EndPublicSubtree,
    }
}

fn states_of(account: &Account) -> States {
    account.data.actor_states.iter().map(|(p, s)| (*p.value(), s.to_vec())).collect::<BTreeMap<_, _>>()
}

/// One proof per holder at a time, across processes of this wallet home (§4.5: the holding is
/// locked from the start of proving until inclusion).
struct HolderLock(PathBuf);

impl HolderLock {
    fn take(holder: AccountId) -> Result<Self> {
        let dir = fetch_persistent_storage_path()?.with_file_name("gate-locks");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(holder.to_string());
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| anyhow!("ACCOUNT_BUSY: {holder} has a proof in progress ({})", path.display()))?;
        Ok(Self(path))
    }
}

impl Drop for HolderLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn random32() -> Result<[u8; 32]> {
    use std::io::Read as _;
    let mut out = [0_u8; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut out)?;
    Ok(out)
}

fn total_memory() -> u64 {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|m| m.lines().find(|l| l.starts_with("MemTotal:")).map(str::to_owned))
            .and_then(|l| l.split_whitespace().nth(1).and_then(|k| k.parse::<u64>().ok()))
            .map_or(0, |kib| kib * 1024)
    }
}

trait DecodeOpt {
    fn decode_opt(bytes: &[u8]) -> Option<NftState>;
}

impl DecodeOpt for NftState {
    /// `NftState::decode` panics on bad bytes; this does not.
    fn decode_opt(bytes: &[u8]) -> Option<NftState> {
        if bytes.is_empty() {
            None
        } else {
            borsh::from_slice(bytes).ok()
        }
    }
}

// ---------------------------------------------------------------------------------------------
// act, verify, gated
// ---------------------------------------------------------------------------------------------

async fn consumer_nonce(wallet: &Wallet, actor: AccountId, consumer: AccountId) -> Result<u64> {
    let state = public_actor_state(wallet, actor, consumer).await?;
    if state.is_empty() {
        return Ok(0);
    }
    let s: ConsumerState = borsh::from_slice(&state).context("the consumer state does not decode")?;
    Ok(s.nonce)
}

async fn act(wallet: &mut Wallet, cfg: &GateConfig, actor: &str, slot: &str, payer: Option<&str>) -> Result<()> {
    let consumer = cfg.consumer.ok_or_else(|| anyhow!("config: consumer missing"))?;
    let actor = resolve(wallet, actor)?;
    let payer = payer.map(|p| resolve(wallet, p)).transpose()?.unwrap_or(actor);
    let slot = gate_verify::unhex32(slot).map_err(|e| anyhow!(e))?;
    let before = consumer_nonce(wallet, actor, consumer).await?;
    let accounts = vec![
        identity(wallet, actor).select_program_actor_state(consumer),
        AccountIdentity::PublicPda { program: cfg.gate, seed: PdaSeed::new(slot) }.select_program_actor_state(cfg.gate).receiving(),
    ];
    let bytes = borsh::to_vec(&ConsumerMessage::Act { slot })?;
    println!("Act by {actor} at consumer nonce {before} on slot {}", gate_verify::hex(&slot));
    let hash = wallet.send_pub_tx_paid_by(accounts, 0, bytes, Some(payer)).await.map_err(|e| anyhow!("{e}"))?;
    finish(wallet, hash).await?;
    let after = consumer_nonce(wallet, actor, consumer).await?;
    println!("gated: consumer nonce {before} -> {after}; tx {hash}");
    Ok(())
}

async fn verify(
    wallet: &mut Wallet,
    cfg: &GateConfig,
    v: VerifyArgs,
    url: &url::Url,
    client: &GuardedClient,
    events_url: Option<&str>,
) -> Result<()> {
    use gate_verify::ChainReader as _;

    let slot = gate_verify::unhex32(&v.slot).map_err(|e| anyhow!(e))?;
    let collection = match &v.collection { Some(c) => resolve(wallet, c)?, None => cfg.collection.ok_or_else(|| anyhow!("give --collection"))? };
    let pinned = gate_verify::Pinned { gate: *cfg.gate.value(), nft_program: *cfg.nft_program.value(), collection: *collection.value(), zone_id: cfg.zone_id };
    enum Kind { Offchain([u8; 32]), Onchain(gate_verify::OnchainUse) }
    let kind = match (&v.caller_identity, &v.tx_hash) {
        (Some(id), None) => {
            let nonce = gate_verify::unhex32(v.app_nonce.as_deref().ok_or_else(|| anyhow!("give --app-nonce"))?).map_err(|e| anyhow!(e))?;
            Kind::Offchain(gate_verify::offchain_context(&pinned, &gate_verify::OffchainUse {
                caller_identity: id.as_bytes(), purpose: v.purpose.as_deref().unwrap_or("").as_bytes(), nonce,
            }))
        }
        (None, Some(tx)) => {
            let consumer = cfg.consumer.ok_or_else(|| anyhow!("config: consumer missing"))?;
            Kind::Onchain(gate_verify::OnchainUse {
                consumer: *consumer.value(),
                submission_id: cfg.submission_id,
                tx_hash: gate_verify::unhex32(tx).map_err(|e| anyhow!(e))?,
            })
        }
        _ => bail!("give either --caller-identity or --tx-hash"),
    };
    // Read first (async), then run the library's synchronous check over what was read. Each
    // actor state is keyed by (account, program): the slot's fact, and for an on-chain use the
    // acting account's consumer state (before 2026-11-20 every read returned the fact).
    let pda = AccountId::new(gate_core::slot_pda::<Sha2>(cfg.gate.value(), &slot));
    let mut states = vec![(
        (*pda.value(), *cfg.gate.value()),
        public_actor_state(wallet, pda, cfg.gate).await.map_err(|e| e.to_string()),
    )];
    let now = latest_timestamp(wallet).await.map_err(|e| e.to_string());
    let rpc_for_events = events_url.map(|s| url::Url::parse(s)).transpose()?;
    let events_client = match &rpc_for_events {
        Some(u) => sequencer_service_rpc::SequencerClientBuilder::default().build(u.as_str())?,
        None => client.inner.clone(),
    };
    let events = match &kind {
        Kind::Offchain(_) => Ok(Vec::new()),
        Kind::Onchain(u) => gate_verify::JsonRpcReader(ClientRpc(events_client)).gated_events(&u.tx_hash, &u.consumer),
    };
    // A standalone sequencer has no indexer: decode the Act transaction itself.
    let act = match &kind {
        Kind::Offchain(_) => Ok(None),
        Kind::Onchain(u) => act_transaction(client, HashType(u.tx_hash)).await.map(Some).map_err(|e| e.to_string()),
    };
    if let Ok(Some(a)) = &act {
        let (actor, consumer) = (AccountId::new(a.actor), AccountId::new(a.consumer));
        states.push(((a.actor, a.consumer), public_actor_state(wallet, actor, consumer).await.map_err(|e| e.to_string())));
    }
    let reader = gate_verify::Prefetched { states, now, events: events.clone(), act };
    let proof = gate_verify::ProofObject {
        gate: pinned.gate, slot, not_before: 0, expires_at: 0, tx_hash: None,
    };
    let verdict = match kind {
        Kind::Offchain(context) => gate_verify::verify_context(&reader, &pinned, &slot, &context),
        Kind::Onchain(u) if events.is_err() => {
            println!("getEvents unavailable; verifying from the Act transaction and consumer state");
            gate_verify::verify_onchain_tx(&reader, &pinned, &proof, &u)
        }
        Kind::Onchain(u) => gate_verify::verify_onchain_use(&reader, &pinned, &proof, &u),
    };
    println!("outcome {} ({})", verdict.outcome.name(), verdict.outcome.plain());
    println!("rpc {url} (trusted for this answer); chain time {:?}", verdict.chain_time_ms);
    if let Some(why) = &verdict.error {
        println!("reason {why}");
    }
    if let Some(f) = verdict.fact {
        println!("fact not_before {} expires_at {}", f.not_before, f.expires_at);
    }
    if verdict.outcome != Outcome::Ok {
        std::process::exit(2);
    }
    Ok(())
}

/// Decode a public Act transaction from getTransaction, checking the block's reverted list.
async fn act_transaction(client: &GuardedClient, hash: HashType) -> Result<gate_verify::ActTransaction> {
    use wallet::WalletSequencerClient as _;
    let (tx, block_id) = client.inner.get_transaction(hash).await.map_err(|e| anyhow!("{e}"))?.ok_or_else(|| anyhow!("not included"))?;
    let LeeTransaction::Public(p) = tx else { bail!("not a public transaction") };
    let ConsumerMessage::Act { slot } = borsh::from_slice(&p.message.execution.root.message).context("not an Act")?;
    let block = client.inner.get_block(block_id).await.map_err(|e| anyhow!("{e}"))?.ok_or_else(|| anyhow!("block {block_id} missing"))?;
    let reverted = block.body.transactions.iter().position(|t| t.hash() == hash).is_some_and(|i| block.body.is_reverted(i));
    Ok(gate_verify::ActTransaction {
        actor: *p.message.execution.root.to.account_id.value(),
        consumer: *p.message.execution.root.to.program_account_id.value(),
        slot, applied: !reverted, block_id: Some(block_id),
    })
}

/// A synchronous `RpcJson` over the sequencer's jsonrpsee HTTP client, for the
/// indexer's `getEvents` method (which the typed sequencer API does not expose).
struct ClientRpc(jsonrpsee::http_client::HttpClient);

impl gate_verify::RpcJson for ClientRpc {
    fn call(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
        use jsonrpsee::core::client::ClientT as _;
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.0
                    .request::<serde_json::Value, _>(method, jsonrpsee::rpc_params![params])
                    .await
            })
        })
        .map_err(|e| e.to_string())
    }
}

async fn gated(wallet: &Wallet, tx: &str) -> Result<()> {
    let hash = HashType(gate_verify::unhex32(tx).map_err(|e| anyhow!(e))?);
    let (tx, block_id) = wallet.poll_transaction(hash).await.context("not included")?;
    let LeeTransaction::Public(p) = tx else { bail!("not a public transaction") };
    let root = &p.message.execution.root;
    let ConsumerMessage::Act { slot } = borsh::from_slice(&root.message).context("not an Act")?;
    let block = wallet.get_block(block_id).await?.ok_or_else(|| anyhow!("block {block_id} missing"))?;
    let reverted = block
        .body
        .transactions
        .iter()
        .position(|t| t.hash() == hash)
        .is_some_and(|i| block.body.is_reverted(i));
    println!("gated_action actor {} consumer {} slot {} block {block_id} timestamp {} reverted {reverted}",
        root.to.account_id, root.to.program_account_id, gate_verify::hex(&slot), block.header.timestamp);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// nft setup (the reference NFT program's own messages)
// ---------------------------------------------------------------------------------------------

async fn nft(wallet: &mut Wallet, program: AccountId, cmd: NftCommand) -> Result<()> {
    let opt = |w: &Wallet, p: &Option<String>| p.as_deref().map(|x| resolve(w, x)).transpose();
    match cmd {
        NftCommand::NewDefinition { definition, master, name, supply, payer } => {
            let (definition, master, payer) = (resolve(wallet, &definition)?, resolve(wallet, &master)?, opt(wallet, &payer)?);
            let message = NftMessage::NewDefinition {
                name,
                printable_supply: supply,
                metadata: nft_core::NftMetadata { standard: nft_core::MetadataStandard::Simple, uri: String::new(), creators: String::new() },
                master,
            };
            let accounts = vec![
                identity(wallet, definition).select_program_actor_state(program),
                identity(wallet, master).select_program_actor_state(program),
            ];
            send(wallet, payer, vec![program], accounts, &message).await
        }
        NftCommand::InitHolding { definition, from, payer } => {
            let (definition, from, payer) = (resolve(wallet, &definition)?, resolve(wallet, &from)?, opt(wallet, &payer)?);
            let accounts = vec![
                identity(wallet, from).select_program_actor_state(program),
                mention(wallet, definition).select_program_actor_state(program),
            ];
            send(wallet, payer, vec![program], accounts, &NftMessage::InitializeHolding { definition }).await
        }
        NftCommand::Print { from, to, payer } => {
            let (from, to, payer) = (resolve(wallet, &from)?, resolve(wallet, &to)?, opt(wallet, &payer)?);
            let accounts = vec![
                identity(wallet, from).select_program_actor_state(program),
                mention(wallet, to).select_program_actor_state(program).receiving(),
            ];
            send(wallet, payer, vec![program], accounts, &NftMessage::PrintNft { to }).await
        }
        NftCommand::Transfer { from, to, payer } => {
            let (from, to, payer) = (resolve(wallet, &from)?, resolve(wallet, &to)?, opt(wallet, &payer)?);
            let accounts = vec![
                identity(wallet, from).select_program_actor_state(program),
                mention(wallet, to).select_program_actor_state(program).receiving(),
            ];
            send(wallet, payer, vec![program], accounts, &NftMessage::Transfer { to }).await
        }
        NftCommand::Read { account } => {
            let account = resolve(wallet, &account)?;
            let state = if wallet.resolve_private_account(account).is_some() {
                wallet.sync_to_latest_block().await?;
                wallet.get_account_private(account).map(|a| a.data.actor_state(program).to_vec()).unwrap_or_default()
            } else {
                public_actor_state(wallet, account, program).await?
            };
            println!("nft state of {account}: {} bytes {} = {:?}", state.len(), gate_verify::hex(&state), NftState::decode_opt(&state));
            Ok(())
        }
        NftCommand::Pending { payer } => {
            let payer = opt(wallet, &payer)?;
            if let Some(p) = payer {
                wallet.set_private_fee_payer(Some(p));
            }
            let catalog = catalog(wallet, &[program]).await?;
            let pending: Vec<_> = wallet
                .owned_pending_messages()
                .await?
                .into_iter()
                .filter(|m| m.body.to.program_account_id == program)
                .collect();
            println!("{} pending NFT message(s)", pending.len());
            for message in pending {
                let (hash, _) = wallet
                    .receive_pending_message_with(message, Vec::new(), &catalog, None, AsyncDelivery::default())
                    .await
                    .map_err(|e| anyhow!("{e}"))?;
                finish(wallet, hash).await?;
            }
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------------------------
// helpers, after nft-drive-v2
// ---------------------------------------------------------------------------------------------

/// A wallet label or an id (base58 or 0x-hex, `Public/`/`Private/` tolerated). Labels win.
fn resolve(wallet: &Wallet, raw: &str) -> Result<AccountId> {
    let raw = raw.trim();
    if let Some(AccountIdWithPrivacy::Public(id) | AccountIdWithPrivacy::Private(id)) = wallet.storage().resolve_label(&Label::new(raw)) {
        return Ok(id);
    }
    parse_id(raw)?.ok_or_else(|| anyhow!("'{raw}' is not a wallet label or an account id"))
}

fn parse_id(raw: &str) -> Result<Option<AccountId>> {
    let t = raw.trim();
    let t = t.strip_prefix("Public/").or_else(|| t.strip_prefix("Private/")).unwrap_or(t);
    if let Some(h) = t.strip_prefix("0x") {
        return Ok(gate_verify::unhex32(h).ok().map(AccountId::new));
    }
    Ok(gate_verify::b58decode32(t).ok().map(AccountId::new))
}

fn identity(wallet: &Wallet, account: AccountId) -> AccountIdentity {
    if let Some(private) = wallet.resolve_private_account(account) {
        return private;
    }
    if wallet.get_account_public_signing_key(account).is_some() {
        AccountIdentity::Public(account)
    } else {
        AccountIdentity::PublicNoSign(account)
    }
}

fn mention(wallet: &Wallet, account: AccountId) -> AccountIdentity {
    wallet.resolve_private_account(account).unwrap_or(AccountIdentity::PublicNoSign(account))
}

async fn catalog(wallet: &Wallet, programs: &[AccountId]) -> Result<ProgramCatalog> {
    use wallet::program_facades::program_loader::ProgramLoader;
    let mut entries = Vec::new();
    for program in programs {
        let deployed = ProgramLoader(wallet).program(*program).await?.ok_or_else(|| anyhow!("{program} is not a deployed program"))?;
        entries.push((*program, deployed));
    }
    Ok(entries.into_iter().collect())
}

async fn send<T: borsh::BorshSerialize>(
    wallet: &mut Wallet,
    payer: Option<AccountId>,
    programs: Vec<AccountId>,
    accounts: Vec<AccountMention>,
    message: &T,
) -> Result<()> {
    let bytes = borsh::to_vec(message)?;
    let private = accounts.iter().any(|m| m.identity.is_private());
    let catalog = catalog(wallet, &programs).await?;
    let hash = if private {
        wallet
            .send_tx_with_pre_check(accounts, 0, bytes, &catalog, None, AsyncDelivery::default(), payer, |_| Ok(()))
            .await
            .map_err(|e| anyhow!("{e}"))?
            .0
    } else {
        wallet.send_pub_tx_paid_by(accounts, 0, bytes, payer).await.map_err(|e| anyhow!("{e}"))?
    };
    finish(wallet, hash).await.map(|_| ())
}

/// Waits for inclusion, refuses a reverted transaction, follows a private one's notes.
async fn finish(wallet: &mut Wallet, hash: HashType) -> Result<u64> {
    println!("submitted {hash}");
    let (tx, block_id) = wallet.poll_transaction(hash).await.context("not included")?;
    let block = wallet.get_block(block_id).await?.ok_or_else(|| anyhow!("block {block_id} is missing"))?;
    let reverted = block
        .body
        .transactions
        .iter()
        .position(|t| t.hash() == hash)
        .is_some_and(|i| block.body.is_reverted(i));
    if let LeeTransaction::PrivacyPreserving(tx) = &tx {
        let key_chain = wallet.storage_mut().key_chain_mut();
        let mut index = key_chain.build_latest_nullifier_index();
        key_chain.follow_transaction(&tx.message, reverted, false, &mut index);
        wallet.store_persistent_data()?;
    }
    if reverted {
        bail!("{hash} was included in block {block_id} but reverted");
    }
    println!("applied in block {block_id} at {}", block.header.timestamp);
    Ok(block_id)
}

async fn public_actor_state(wallet: &Wallet, account: AccountId, program: AccountId) -> Result<Vec<u8>> {
    let view = wallet.get_account_view(Actor::new(account, program)).await?;
    Ok(view.map_or_else(Vec::new, |found| found.data.actor_state(program).to_vec()))
}

async fn latest_timestamp(wallet: &Wallet) -> Result<u64> {
    let id = wallet.get_last_block_id().await?;
    let block = wallet.get_block(id).await?.ok_or_else(|| anyhow!("block {id} missing"))?;
    Ok(block.header.timestamp)
}

async fn read_header(wallet: &Wallet, program: AccountId) -> Result<([u32; 8], bool, AccountId)> {
    let account = wallet
        .get_account_view(Actor::new(program, PROGRAM_LOADER_ACCOUNT_ID))
        .await?
        .ok_or_else(|| anyhow!("{program} has no header"))?;
    let header = ProgramHeader::from_loader_actor_state(account.data.actor_state(PROGRAM_LOADER_ACCOUNT_ID))
        .ok_or_else(|| anyhow!("{program} is not a program header"))?;
    Ok((header.image_id, header.immutable, header.program_first_segment))
}

/// An image id's bytes, as `AccountId::from_builtin_program` lays the words out.
fn image_bytes(id: &[u32; 8]) -> [u8; 32] {
    let mut out = [0_u8; 32];
    for (i, w) in id.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

fn words(bytes: &[u8; 32]) -> [u32; 8] {
    let mut out = [0_u32; 8];
    for (i, w) in out.iter_mut().enumerate() {
        *w = u32::from_le_bytes([bytes[i * 4], bytes[i * 4 + 1], bytes[i * 4 + 2], bytes[i * 4 + 3]]);
    }
    out
}

async fn run_wallet(args: Vec<String>, overrides: WalletConfigOverrides) -> Result<()> {
    use clap::Parser as _;
    use wallet::cli::{execute_subcommand, Args};
    let Args { command, dry_run, allow_revert, gas_limit, max_fee, tip, fee_headroom, fee_payer, refund_to, .. } =
        Args::parse_from(std::iter::once("gate-cli wallet".to_owned()).chain(args));
    let config_path = fetch_config_path()?;
    let storage_path = fetch_persistent_storage_path()?;
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let command = command.ok_or_else(|| anyhow!("give a wallet command"))?;
    let mut wallet = if storage_path.exists() {
        let password = read_password(&storage_path)?;
        WalletCore::new_update_chain(config_path, storage_path, Some(overrides), &password)?
    } else {
        let password = std::env::var(wallet::PASSWORD_ENV_VAR).unwrap_or_default();
        let (mut wallet, _mnemonic) = WalletCore::new_init_storage(config_path, storage_path, Some(overrides), &password)?;
        wallet.start_at_chain_tip().await.ok();
        wallet.store_persistent_data()?;
        println!("created a new wallet (recovery phrase not shown)");
        wallet
    };
    wallet.set_dry_run(dry_run);
    wallet.set_allow_revert(allow_revert);
    wallet.set_fee_params(wallet::FeeParams { gas_limit, max_fee, tip, headroom_blocks: fee_headroom, payer: fee_payer, refund_to });
    match execute_subcommand(&mut wallet, command).await {
        Err(error) => match error.root_cause().downcast_ref() {
            Some(ExecutionFailureKind::DryRun(report)) => {
                println!("{report}");
                Ok(())
            }
            _ => Err(error),
        },
        Ok(_) => Ok(()),
    }
}

async fn decode_tx(wallet: &Wallet, hash: &str) -> Result<()> {
    let tx_hash = HashType(gate_verify::unhex32(hash).map_err(|e| anyhow!(e))?);
    let (tx, block_id) = wallet.poll_transaction(tx_hash).await.context("not included")?;
    let h = |label: &str, bytes: Vec<u8>| println!("{label} {} {}", bytes.len(), gate_verify::hex(&bytes));
    println!("tx {hash} block {block_id} borsh_len {}", borsh::to_vec(&tx)?.len());
    match &tx {
        LeeTransaction::Public(public) => {
            println!("kind public");
            println!("actors {:?}", public.message.public_context.actors);
            println!("root {:?}", public.message.execution.root.to);
            println!("fee {:?}", public.message.execution.fee);
            h("message", borsh::to_vec(&public.message)?);
        }
        LeeTransaction::PrivacyPreserving(private) => {
            println!("kind private");
            let m = &private.message;
            let e = &m.execution;
            println!("public_context.actors {:?}", m.public_context.actors);
            println!("public_context.authorized {:?}", m.public_context.authorized_accounts);
            h("admission_evidence", borsh::to_vec(&m.admission_evidence)?);
            println!(
                "counts actions {} async {} recovery {} claims {} boundary {}",
                e.private_actions.len(),
                e.async_messages.len(),
                e.recovery_bindings.len(),
                e.program_image_claims.len(),
                e.boundary.len()
            );
            for (i, a) in e.private_actions.iter().enumerate() {
                h(&format!("action[{i}].nullifier"), a.nullifier.to_byte_array().to_vec());
                h(&format!("action[{i}].commitment"), a.commitment.to_byte_array().to_vec());
            }
            for (i, step) in e.boundary.iter().enumerate() {
                println!("boundary[{i}] {step:?}");
                h(&format!("boundary[{i}]"), borsh::to_vec(step)?);
            }
            println!("public_root {:?}", e.public_root);
            println!("validity timestamps [{:?}, {:?})", e.validity.timestamps.start(), e.validity.timestamps.end());
            h("validity", borsh::to_vec(&e.validity)?);
            h("fee", borsh::to_vec(&e.fee)?);
            h("witness_set", borsh::to_vec(&private.witness_set)?);
        }
    }
    Ok(())
}

async fn snapshot(wallet: &mut Wallet) -> Result<()> {
    wallet.sync_to_latest_block().await?;
    let ids: Vec<AccountId> = wallet.storage().key_chain().private_account_ids().map(|(id, _)| id).collect();
    for id in ids {
        let labels: Vec<String> = wallet.storage().labels_for_account(AccountIdWithPrivacy::Private(id)).map(|l| l.to_string()).collect();
        let Some(account) = wallet.private_account_state(id).cloned() else { continue };
        let commitment = lee_core::Commitment::new(&id, &account);
        let next = wallet.storage().key_chain().managed_private_account(id).map(|m| {
            let nsk = m.keys.nullifier_secret_key();
            lee_core::Nullifier::for_account_update(&commitment, &nsk)
        });
        println!(
            "snap {id} labels {labels:?} commitment {} next_nullifier {}",
            gate_verify::hex(&commitment.to_byte_array()),
            next.map_or_else(|| "-".to_owned(), |n| gate_verify::hex(&n.to_byte_array()))
        );
    }
    Ok(())
}
