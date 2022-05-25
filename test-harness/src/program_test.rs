use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use log::*;
use tokio::task::JoinHandle;
use mundis_program_runtime::compute_budget::ComputeBudget;
use mundis_program_runtime::invoke_context::ProcessInstructionWithContext;
use mundis_program_runtime::timings::ExecuteTimings;
use mundis_runtime::bank::Bank;
use mundis_runtime::bank_forks::BankForks;
use mundis_runtime::builtins::Builtin;
use mundis_runtime::commitment::BlockCommitmentCache;
use mundis_runtime::genesis_utils::{create_genesis_config_with_leader_ex, GenesisConfigInfo};
use mundis_sdk::account::{Account, AccountSharedData};
use mundis_sdk::feature_set::FEATURE_NAMES;
use mundis_sdk::fee_calculator::FeeRateGovernor;
use mundis_sdk::genesis_config::{ClusterType, GenesisConfig};
use mundis_sdk::hash::Hash;
use mundis_sdk::native_token::mdis_to_lamports;
use mundis_sdk::poh_config::PohConfig;
use mundis_sdk::pubkey::Pubkey;
use mundis_sdk::rent::Rent;
use mundis_sdk::signature::Keypair;
use mundis_sdk::signer::Signer;
use mundis_vote_program::vote_state::VoteState;
use crate::banks_client::{BanksClient, start_client};
use crate::banks_server::start_local_server;

fn setup_fees(bank: Bank) -> Bank {
    // Realistic fees part 1: Fake a single signature by calling
    // `bank.commit_transactions()` so that the fee in the child bank will be
    // initialized with a non-zero fee.
    assert_eq!(bank.signature_count(), 0);
    bank.commit_transactions(
        &[],     // transactions
        &mut [], // loaded accounts
        vec![],  // transaction execution results
        0,       // executed tx count
        0,       // executed with failure output tx count
        1,       // signature count
        &mut ExecuteTimings::default(),
    );
    assert_eq!(bank.signature_count(), 1);

    // Advance beyond slot 0 for a slightly more realistic test environment
    let bank = Arc::new(bank);
    let bank = Bank::new_from_parent(&bank, bank.collector_id(), bank.slot() + 1);
    debug!("Bank slot: {}", bank.slot());

    // Realistic fees part 2: Tick until a new blockhash is produced to pick up the
    // non-zero fees
    let last_blockhash = bank.last_blockhash();
    while last_blockhash == bank.last_blockhash() {
        bank.register_tick(&Hash::new_unique());
    }

    // Make sure a fee is now required
    let lamports_per_signature = bank.get_lamports_per_signature();
    assert_ne!(lamports_per_signature, 0);

    bank
}

pub struct ProgramTest {
    accounts: Vec<(Pubkey, AccountSharedData)>,
    builtins: Vec<Builtin>,
    compute_max_units: Option<u64>,
    deactivate_feature_set: HashSet<Pubkey>,
}

impl Default for ProgramTest {
    fn default() -> Self {
        mundis_logger::setup_with_default(
             "mundis_runtime::message_processor=debug,\
             mundis_runtime::system_instruction_processor=trace,\
             mundis_runtime::accounts=info,\
             mundis_program_test=info",
        );
        Self {
            accounts: vec![],
            builtins: vec![],
            compute_max_units: None,
            deactivate_feature_set: HashSet::default(),
        }
    }
}

impl ProgramTest {
    /// Create a `ProgramTest`.
    ///
    /// This is a wrapper around [`default`] and [`add_program`]. See their documentation for more
    /// details.
    ///
    /// [`default`]: #method.default
    pub fn new(
    ) -> Self {
        Self::default()
    }

    /// Override the default maximum compute units
    pub fn set_compute_max_units(&mut self, compute_max_units: u64) {
        self.compute_max_units = Some(compute_max_units);
    }

    /// Add an account to the test environment
    pub fn add_account(&mut self, address: Pubkey, account: Account) {
        self.accounts
            .push((address, AccountSharedData::from(account)));
    }

    /// Add an account to the test environment with the account data in the provided as a base 64
    /// string
    pub fn add_account_with_base64_data(
        &mut self,
        address: Pubkey,
        lamports: u64,
        owner: Pubkey,
        data_base64: &str,
    ) {
        self.add_account(
            address,
            Account {
                lamports,
                data: base64::decode(data_base64)
                    .unwrap_or_else(|err| panic!("Failed to base64 decode: {}", err)),
                owner,
                executable: false,
                rent_epoch: 0,
            },
        );
    }

    /// Add a builtin program to the test environment.
    ///
    /// Note that builtin programs are responsible for their own `stable_log` output.
    pub fn add_builtin_program(
        &mut self,
        program_name: &str,
        program_id: Pubkey,
        process_instruction: ProcessInstructionWithContext,
    ) {
        info!("\"{}\" builtin program", program_name);
        self.builtins
            .push(Builtin::new(program_name, program_id, process_instruction));
    }

    /// Deactivate a runtime feature.
    ///
    /// Note that all features are activated by default.
    pub fn deactivate_feature(&mut self, feature_id: Pubkey) {
        self.deactivate_feature_set.insert(feature_id);
    }

    fn setup_bank(
        &self,
    ) -> (
        Arc<RwLock<BankForks>>,
        Arc<RwLock<BlockCommitmentCache>>,
        Hash,
        GenesisConfigInfo,
    ) {
        let rent = Rent::default();
        let fee_rate_governor = FeeRateGovernor::default();
        let bootstrap_validator_pubkey = Pubkey::new_unique();
        let bootstrap_validator_stake_lamports =
            rent.minimum_balance(VoteState::size_of()) + mdis_to_lamports(1_000_000.0);

        let mint_keypair = Keypair::new();
        let voting_keypair = Keypair::new();

        let mut genesis_config = create_genesis_config_with_leader_ex(
            mdis_to_lamports(1_000_000.0),
            &mint_keypair.pubkey(),
            &bootstrap_validator_pubkey,
            &voting_keypair.pubkey(),
            &Pubkey::new_unique(),
            bootstrap_validator_stake_lamports,
            42,
            fee_rate_governor,
            rent,
            ClusterType::Development,
            vec![],
        );

        // Remove features tagged to deactivate
        for deactivate_feature_pk in &self.deactivate_feature_set {
            if FEATURE_NAMES.contains_key(deactivate_feature_pk) {
                match genesis_config.accounts.remove(deactivate_feature_pk) {
                    Some(_) => debug!("Feature for {:?} deactivated", deactivate_feature_pk),
                    None => warn!(
                        "Feature {:?} set for deactivation not found in genesis_config account list, ignored.",
                        deactivate_feature_pk
                    ),
                }
            } else {
                warn!(
                    "Feature {:?} set for deactivation is not a known Feature public key",
                    deactivate_feature_pk
                );
            }
        }

        let target_tick_duration = Duration::from_micros(100);
        genesis_config.poh_config = PohConfig::new_sleep(target_tick_duration);
        debug!("Payer address: {}", mint_keypair.pubkey());
        debug!("Genesis config: {}", genesis_config);

        let mut bank = Bank::new_for_tests(&genesis_config);

        // User-supplied additional builtins
        for builtin in self.builtins.iter() {
            bank.add_builtin(
                &builtin.name,
                &builtin.id,
                builtin.process_instruction_with_context,
            );
        }

        for (address, account) in self.accounts.iter() {
            if bank.get_account(address).is_some() {
                info!("Overriding account at {}", address);
            }
            bank.store_account(address, account);
        }
        bank.set_capitalization();
        if let Some(max_units) = self.compute_max_units {
            bank.set_compute_budget(Some(ComputeBudget {
                max_units,
                ..ComputeBudget::default()
            }));
        }

        let bank = setup_fees(bank);
        let slot = bank.slot();
        let last_blockhash = bank.last_blockhash();
        let bank_forks = Arc::new(RwLock::new(BankForks::new(bank)));
        let block_commitment_cache = Arc::new(RwLock::new(
            BlockCommitmentCache::new_for_tests_with_slots(slot, slot),
        ));

        (
            bank_forks,
            block_commitment_cache,
            last_blockhash,
            GenesisConfigInfo {
                genesis_config,
                mint_keypair,
                voting_keypair,
                validator_pubkey: bootstrap_validator_pubkey,
            },
        )
    }

    pub async fn start(self) -> (BanksClient, Keypair, Hash) {
        let (bank_forks, block_commitment_cache, last_blockhash, gci) = self.setup_bank();
        let target_tick_duration = gci.genesis_config.poh_config.target_tick_duration;
        let transport = start_local_server(
            bank_forks.clone(),
            block_commitment_cache.clone(),
            target_tick_duration,
        )
            .await;
        let banks_client = start_client(transport)
            .await
            .unwrap_or_else(|err| panic!("Failed to start banks client: {}", err));

        // Run a simulated PohService to provide the client with new blockhashes.  New blockhashes
        // are required when sending multiple otherwise identical transactions in series from a
        // test
        tokio::spawn(async move {
            loop {
                bank_forks
                    .read()
                    .unwrap()
                    .working_bank()
                    .register_tick(&Hash::new_unique());
                tokio::time::sleep(target_tick_duration).await;
            }
        });

        (banks_client, gci.mint_keypair, last_blockhash)
    }

    /// Start the test client
    ///
    /// Returns a `BanksClient` interface into the test environment as well as a payer `Keypair`
    /// with MUNDIS for sending transactions
    pub async fn start_with_context(self) -> ProgramTestContext {
        let (bank_forks, block_commitment_cache, last_blockhash, gci) = self.setup_bank();
        let target_tick_duration = gci.genesis_config.poh_config.target_tick_duration;
        let transport = start_local_server(
            bank_forks.clone(),
            block_commitment_cache.clone(),
            target_tick_duration,
        )
            .await;
        let banks_client = start_client(transport)
            .await
            .unwrap_or_else(|err| panic!("Failed to start banks client: {}", err));

        ProgramTestContext::new(
            bank_forks,
            block_commitment_cache,
            banks_client,
            last_blockhash,
            gci,
        )
    }
}

struct DroppableTask<T>(Arc<AtomicBool>, JoinHandle<T>);

impl<T> Drop for DroppableTask<T> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

#[allow(dead_code)]
pub struct ProgramTestContext {
    pub banks_client: BanksClient,
    pub last_blockhash: Hash,
    pub payer: Keypair,
    genesis_config: GenesisConfig,
    bank_forks: Arc<RwLock<BankForks>>,
    block_commitment_cache: Arc<RwLock<BlockCommitmentCache>>,
    _bank_task: DroppableTask<()>,
}

impl ProgramTestContext {
    fn new(
        bank_forks: Arc<RwLock<BankForks>>,
        block_commitment_cache: Arc<RwLock<BlockCommitmentCache>>,
        banks_client: BanksClient,
        last_blockhash: Hash,
        genesis_config_info: GenesisConfigInfo,
    ) -> Self {
        // Run a simulated PohService to provide the client with new blockhashes.  New blockhashes
        // are required when sending multiple otherwise identical transactions in series from a
        // test
        let running_bank_forks = bank_forks.clone();
        let target_tick_duration = genesis_config_info
            .genesis_config
            .poh_config
            .target_tick_duration;
        let exit = Arc::new(AtomicBool::new(false));
        let bank_task = DroppableTask(
            exit.clone(),
            tokio::spawn(async move {
                loop {
                    if exit.load(Ordering::Relaxed) {
                        break;
                    }
                    running_bank_forks
                        .read()
                        .unwrap()
                        .working_bank()
                        .register_tick(&Hash::new_unique());
                    tokio::time::sleep(target_tick_duration).await;
                }
            }),
        );

        Self {
            banks_client,
            last_blockhash,
            payer: genesis_config_info.mint_keypair,
            genesis_config: genesis_config_info.genesis_config,
            bank_forks,
            block_commitment_cache,
            _bank_task: bank_task,
        }
    }
}