use borsh::BorshDeserialize;
use mundis_program_runtime::invoke_context::InvokeContext;
use mundis_sdk::account::ReadableAccount;
use mundis_sdk::borsh::try_from_slice_unchecked;
use mundis_sdk::keyed_account::KeyedAccount;
use mundis_sdk::program_pack::{IsInitialized, Pack};
use mundis_sdk::rent::Rent;
use crate::error::VaultError;
use crate::{InstructionError, Pubkey};
use crate::state::{Key, Vault};

///TokenTransferParams
pub struct TokenTransferParams<'a: 'b, 'b> {
    /// source
    pub source: KeyedAccount<'a>,
    /// destination
    pub destination: KeyedAccount<'a>,
    /// amount
    pub amount: u64,
    /// authority
    pub authority: KeyedAccount<'a>,
    /// authority_signer_seeds
    pub authority_signer_seeds: &'b [&'b [u8]],
}

/// assert initialized account
pub fn assert_initialized<T: Pack + IsInitialized> (
    account_info: &KeyedAccount,
) -> Result<T, InstructionError> {
    let account: T = T::unpack_unchecked(&account_info.try_account_ref()?.data())?;
    if !account.is_initialized() {
        Err(VaultError::Uninitialized.into())
    } else {
        Ok(account)
    }
}

pub fn assert_rent_exempt(rent: &Rent, account_info: &KeyedAccount) -> Result<(), InstructionError> {
    if !rent.is_exempt(account_info.lamports()?, account_info.data_len()?) {
        Err(VaultError::NotRentExempt.into())
    } else {
        Ok(())
    }
}

pub fn assert_owned_by(account: &KeyedAccount, owner: &Pubkey) -> Result<(), InstructionError> {
    if account.owner()? != *owner {
        Err(VaultError::IncorrectOwner.into())
    } else {
        Ok(())
    }
}

pub fn assert_vault_authority_correct(
    vault: &Vault,
    vault_authority_info: &KeyedAccount,
) -> Result<(), InstructionError> {
    if vault_authority_info.signer_key().is_none() {
        return Err(VaultError::AuthorityIsNotSigner.into());
    }

    if *vault_authority_info.signer_key().unwrap() != vault.authority {
        return Err(VaultError::AuthorityDoesNotMatch.into());
    }
    Ok(())
}

#[inline(always)]
pub fn create_or_allocate_account_raw<'a>(
    program_id: Pubkey,
    new_account_info: &KeyedAccount<'a>,
    rent: &Rent,
    payer_info: &KeyedAccount<'a>,
    size: usize,
    signer_seeds: &[&[u8]],
) -> Result<(), InstructionError> {
    Ok(())
}

/// Issue a spl_token `Transfer` instruction.
#[inline(always)]
pub fn token_transfer(
    params: TokenTransferParams<'_, '_>,
    invoke_context: &mut InvokeContext,
) -> Result<(), InstructionError> {
    let TokenTransferParams {
        source,
        destination,
        authority,
        amount,
        authority_signer_seeds,
    } = params;
    let result = invoke_context.native_invoke(
        mundis_token_program::token_instruction::transfer(
            &mundis_token_program::id(),
            source.unsigned_key(),
            destination.unsigned_key(),
            authority.unsigned_key(),
            &[],
            amount
        ).unwrap(),
        &[]
    );
    result.map_err(|_| VaultError::TokenTransferFailed.into())
}


pub fn try_from_slice_checked<T: BorshDeserialize>(
    data: &[u8],
    data_type: Key,
    data_size: usize,
) -> Result<T, InstructionError> {
    if (data.len() == 0 || data[0] != data_type as u8 && data[0] != Key::Uninitialized as u8) || data.len() != data_size
    {
        return Err(VaultError::DataTypeMismatch.into());
    }

    try_from_slice_unchecked(data)
        .map_err(|_| VaultError::DataTypeMismatch.into())
}