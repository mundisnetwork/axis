use {
    jsonrpc_core::{Error, Result},
    mundis_account_decoder::{
        parse_account_data::AccountAdditionalData,
        parse_token::{
            get_token_account_mint, mundis_token_native_mint, mundis_token_native_mint_program_id,
        },
        UiAccount, UiAccountData, UiAccountEncoding,
    },
    mundis_client::rpc_response::RpcKeyedAccount,
    mundis_runtime::bank::Bank,
    mundis_sdk::{
        account::{AccountSharedData, ReadableAccount},
        pubkey::Pubkey,
    },
    std::{collections::HashMap, sync::Arc},
};
use mundis_token_program::state::Mint;

pub fn get_parsed_token_account(
    bank: Arc<Bank>,
    pubkey: &Pubkey,
    account: AccountSharedData,
) -> UiAccount {
    let additional_data = get_token_account_mint(account.data())
        .and_then(|mint_pubkey| get_mint_owner_and_decimals(&bank, &mint_pubkey).ok())
        .map(|(_, decimals)| AccountAdditionalData {
            token_decimals: Some(decimals),
        });

    UiAccount::encode(
        pubkey,
        &account,
        UiAccountEncoding::JsonParsed,
        additional_data,
        None,
    )
}

pub fn get_parsed_token_accounts<I>(
    bank: Arc<Bank>,
    keyed_accounts: I,
) -> impl Iterator<Item = RpcKeyedAccount>
where
    I: Iterator<Item = (Pubkey, AccountSharedData)>,
{
    let mut mint_decimals: HashMap<Pubkey, u8> = HashMap::new();
    keyed_accounts.filter_map(move |(pubkey, account)| {
        let additional_data = get_token_account_mint(account.data()).map(|mint_pubkey| {
            let token_decimals = mint_decimals.get(&mint_pubkey).cloned().or_else(|| {
                let (_, decimals) = get_mint_owner_and_decimals(&bank, &mint_pubkey).ok()?;
                mint_decimals.insert(mint_pubkey, decimals);
                Some(decimals)
            });
            AccountAdditionalData { token_decimals }
        });

        let maybe_encoded_account = UiAccount::encode(
            &pubkey,
            &account,
            UiAccountEncoding::JsonParsed,
            additional_data,
            None,
        );

        if let UiAccountData::Json(_) = &maybe_encoded_account.data {
            Some(RpcKeyedAccount {
                pubkey: pubkey.to_string(),
                account: maybe_encoded_account,
            })
        } else {
            None
        }
    })
}

/// Analyze a mint Pubkey that may be the native_mint and get the mint-account owner (token
/// program_id) and decimals
pub fn get_mint_owner_and_decimals(bank: &Arc<Bank>, mint: &Pubkey) -> Result<(Pubkey, u8)> {
    if mint == &mundis_token_native_mint() {
        Ok((
            mundis_token_native_mint_program_id(),
            mundis_token_program::native_mint::DECIMALS,
        ))
    } else {
        let mint_account = bank.get_account(mint).ok_or_else(|| {
            Error::invalid_params("Invalid param: could not find mint".to_string())
        })?;
        let decimals = get_mint_decimals(mint_account.data())?;
        Ok((*mint_account.owner(), decimals))
    }
}

fn get_mint_decimals(data: &[u8]) -> Result<u8> {
    Mint::unpack(data)
        .map_err(|_| {
            Error::invalid_params("Invalid param: Token mint could not be unpacked".to_string())
        })
        .map(|mint| mint.decimals)
}
