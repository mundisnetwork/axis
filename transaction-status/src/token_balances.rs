use {
    crate::TransactionTokenBalance,
    mundis_account_decoder::parse_token::{
        is_known_anima_token_id, anima_token_native_mint,
        token_amount_to_ui_amount, UiTokenAmount,
    },
    mundis_measure::measure::Measure,
    mundis_metrics::datapoint_debug,
    mundis_runtime::{bank::Bank, transaction_batch::TransactionBatch},
    mundis_sdk::{account::ReadableAccount, pubkey::Pubkey},
    std::collections::HashMap,
};
use mundis_token_program::state::{Mint, Account as TokenAccount};

pub type TransactionTokenBalances = Vec<Vec<TransactionTokenBalance>>;

pub struct TransactionTokenBalancesSet {
    pub pre_token_balances: TransactionTokenBalances,
    pub post_token_balances: TransactionTokenBalances,
}

impl TransactionTokenBalancesSet {
    pub fn new(
        pre_token_balances: TransactionTokenBalances,
        post_token_balances: TransactionTokenBalances,
    ) -> Self {
        assert_eq!(pre_token_balances.len(), post_token_balances.len());
        Self {
            pre_token_balances,
            post_token_balances,
        }
    }
}

fn get_mint_decimals(bank: &Bank, mint: &Pubkey) -> Option<u8> {
    if mint == &anima_token_native_mint() {
        Some(mundis_token_program::native_mint::DECIMALS)
    } else {
        let mint_account = bank.get_account(mint)?;

        if !is_known_anima_token_id(mint_account.owner()) {
            return None;
        }

        let decimals = Mint::unpack(mint_account.data())
            .map(|mint| mint.decimals)
            .ok()?;

        Some(decimals)
    }
}

pub fn collect_token_balances(
    bank: &Bank,
    batch: &TransactionBatch,
    mint_decimals: &mut HashMap<Pubkey, u8>,
) -> TransactionTokenBalances {
    let mut balances: TransactionTokenBalances = vec![];
    let mut collect_time = Measure::start("collect_token_balances");

    for transaction in batch.sanitized_transactions() {
        let has_token_program = transaction
            .message()
            .account_keys_iter()
            .any(is_known_anima_token_id);

        let mut transaction_balances: Vec<TransactionTokenBalance> = vec![];
        if has_token_program {
            for (index, account_id) in transaction.message().account_keys_iter().enumerate() {
                if transaction.message().is_invoked(index) || is_known_anima_token_id(account_id) {
                    continue;
                }

                if let Some(TokenBalanceData {
                    mint,
                    ui_token_amount,
                    owner,
                }) = collect_token_balance_from_account(bank, account_id, mint_decimals)
                {
                    transaction_balances.push(TransactionTokenBalance {
                        account_index: index as u8,
                        mint,
                        ui_token_amount,
                        owner,
                    });
                }
            }
        }
        balances.push(transaction_balances);
    }
    collect_time.stop();
    datapoint_debug!(
        "collect_token_balances",
        ("collect_time_us", collect_time.as_us(), i64),
    );
    balances
}

#[derive(Debug, PartialEq)]
struct TokenBalanceData {
    mint: String,
    owner: String,
    ui_token_amount: UiTokenAmount,
}

fn collect_token_balance_from_account(
    bank: &Bank,
    account_id: &Pubkey,
    mint_decimals: &mut HashMap<Pubkey, u8>,
) -> Option<TokenBalanceData> {
    let account = bank.get_account(account_id)?;

    if !is_known_anima_token_id(account.owner()) {
        return None;
    }

    let token_account = TokenAccount::unpack(account.data()).ok()?;
    let mint = token_account.mint;

    let decimals = mint_decimals.get(&mint).cloned().or_else(|| {
        let decimals = get_mint_decimals(bank, &mint)?;
        mint_decimals.insert(mint, decimals);
        Some(decimals)
    })?;

    Some(TokenBalanceData {
        mint: token_account.mint.to_string(),
        owner: token_account.owner.to_string(),
        ui_token_amount: token_amount_to_ui_amount(token_account.amount, decimals),
    })
}

#[cfg(test)]
mod test {
    use {
        super::*,
        mundis_sdk::{account::Account, genesis_config::create_genesis_config},
        std::collections::BTreeMap,
    };

    #[test]
    fn test_collect_token_balance_from_account() {
        let (mut genesis_config, _mint_keypair) = create_genesis_config(500);

        // Add a variety of accounts, token and not
        let account = Account::new(42, 55, &Pubkey::new_unique());

        let mint_data = Mint {
            mint_authority: None,
            supply: 4242,
            decimals: 2,
            is_initialized: true,
            freeze_authority: None,
        };
        let mut data = [0; Mint::LEN];
        mint_data.pack(&mut data).unwrap();
        let mint_pubkey = Pubkey::new_unique();
        let mint = Account {
            lamports: 100,
            data: data.to_vec(),
            owner: mundis_sdk::token::program::id(),
            executable: false,
            rent_epoch: 0,
        };
        let other_mint_pubkey = Pubkey::new_unique();
        let other_mint = Account {
            lamports: 100,
            data: data.to_vec(),
            owner: Pubkey::new_unique(),
            executable: false,
            rent_epoch: 0,
        };

        let token_owner = Pubkey::new_unique();
        let token_data = TokenAccount {
            mint: mint_pubkey,
            owner: token_owner,
            amount: 42,
            delegate: None,
            state: mundis_token_program::state::AccountState::Initialized,
            is_native: true,
            delegated_amount: 0,
            close_authority: None,
        };
        let mut data = [0; TokenAccount::LEN];
        token_data.pack(&mut data).unwrap();

        let anima_token_account = Account {
            lamports: 100,
            data: data.to_vec(),
            owner: mundis_sdk::token::program::id(),
            executable: false,
            rent_epoch: 0,
        };
        let other_account = Account {
            lamports: 100,
            data: data.to_vec(),
            owner: Pubkey::new_unique(),
            executable: false,
            rent_epoch: 0,
        };

        let other_mint_data = TokenAccount {
            mint: other_mint_pubkey,
            owner: token_owner,
            amount: 42,
            delegate: None,
            state: mundis_token_program::state::AccountState::Initialized,
            is_native: true,
            delegated_amount: 0,
            close_authority: None,
        };
        let mut data = [0; TokenAccount::LEN];
        other_mint_data.pack(&mut data).unwrap();

        let other_mint_token_account = Account {
            lamports: 100,
            data: data.to_vec(),
            owner: mundis_sdk::token::program::id(),
            executable: false,
            rent_epoch: 0,
        };

        let mut accounts = BTreeMap::new();

        let account_pubkey = Pubkey::new_unique();
        accounts.insert(account_pubkey, account);
        accounts.insert(mint_pubkey, mint);
        accounts.insert(other_mint_pubkey, other_mint);
        let anima_token_account_pubkey = Pubkey::new_unique();
        accounts.insert(anima_token_account_pubkey, anima_token_account);
        let other_account_pubkey = Pubkey::new_unique();
        accounts.insert(other_account_pubkey, other_account);
        let other_mint_account_pubkey = Pubkey::new_unique();
        accounts.insert(other_mint_account_pubkey, other_mint_token_account);

        genesis_config.accounts = accounts;

        let bank = Bank::new_for_tests(&genesis_config);
        let mut mint_decimals = HashMap::new();

        assert_eq!(
            collect_token_balance_from_account(&bank, &account_pubkey, &mut mint_decimals),
            None
        );

        assert_eq!(
            collect_token_balance_from_account(&bank, &mint_pubkey, &mut mint_decimals),
            None
        );

        assert_eq!(
            collect_token_balance_from_account(
                &bank,
                &anima_token_account_pubkey,
                &mut mint_decimals
            ),
            Some(TokenBalanceData {
                mint: mint_pubkey.to_string(),
                owner: token_owner.to_string(),
                ui_token_amount: UiTokenAmount {
                    ui_amount: Some(0.42),
                    decimals: 2,
                    amount: "42".to_string(),
                    ui_amount_string: "0.42".to_string(),
                }
            })
        );

        assert_eq!(
            collect_token_balance_from_account(&bank, &other_account_pubkey, &mut mint_decimals),
            None
        );

        assert_eq!(
            collect_token_balance_from_account(
                &bank,
                &other_mint_account_pubkey,
                &mut mint_decimals
            ),
            None
        );
    }
}
