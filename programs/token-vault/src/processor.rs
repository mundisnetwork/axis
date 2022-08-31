use std::cell::RefCell;
use mundis_program_runtime::ic_msg;
use borsh::{BorshDeserialize, BorshSerialize};

use mundis_program_runtime::invoke_context::InvokeContext;
use mundis_sdk::account::WritableAccount;
use mundis_sdk::decode_error::PrintInstructionError;
use mundis_sdk::keyed_account::{keyed_account_at_index, KeyedAccount, next_keyed_account};
use mundis_sdk::program_utils::limited_deserialize;
use mundis_sdk::instruction::InstructionError;
use mundis_sdk::pubkey::Pubkey;
use mundis_sdk::rent::Rent;
use mundis_token_program::state::{Mint, TokenAccount};
use crate::error::VaultError;
use crate::instruction::VaultInstruction;
use crate::state::{ExternalPriceAccount, Key, MAX_SAFETY_DEPOSIT_SIZE, PREFIX, SafetyDepositBox, Vault, VaultState};
use crate::utils::{assert_initialized, assert_owned_by, assert_rent_exempt, assert_vault_authority_correct, create_or_allocate_account_raw, token_transfer, TokenTransferParams};

pub fn process_instruction(
    first_instruction_account: usize,
    data: &[u8],
    invoke_context: &mut InvokeContext,
) -> Result<(), InstructionError> {
    if let Err(error) = Processor::process(first_instruction_account, data, invoke_context) {
        // catch the error so we can print it
        error.print::<VaultError>();
        return Err(error);
    }
    Ok(())
}

pub struct Processor {}

impl Processor {
    fn process(
        first_instruction_account: usize,
        data: &[u8],
        invoke_context: &mut InvokeContext,
    ) -> Result<(), InstructionError> {
        match limited_deserialize(data)? {
            VaultInstruction::InitVault {
                allow_further_share_creation
            } => {
                ic_msg!(invoke_context, "Instruction: InitVault");
                Self::process_init_vault(invoke_context, first_instruction_account, allow_further_share_creation)
            }
            VaultInstruction::ActivateVault {
                number_of_shares
            } => {
                ic_msg!(invoke_context, "Instruction: ActivateVault");
                Ok(())
            }
            VaultInstruction::CombineVault => {
                ic_msg!(invoke_context, "Instruction: CombineVault");
                Ok(())
            }
            VaultInstruction::AddTokenToInactiveVault {
                amount
            } => {
                ic_msg!(invoke_context, "Instruction: AddTokenToInactiveVault");
                Self::process_add_token_to_inactivated_vault(invoke_context, first_instruction_account, amount)
            }
        }
    }

    pub fn process_add_token_to_inactivated_vault(
        invoke_context: &mut InvokeContext,
        first_instruction_account: usize,
        amount: u64,
    ) -> Result<(), InstructionError> {
        let keyed_accounts= invoke_context.get_keyed_accounts()?;
        let program_id = keyed_account_at_index(keyed_accounts, 0)?.unsigned_key();
        let accounts = &keyed_accounts[first_instruction_account..];

        let account_info_iter = &mut accounts.iter();
        let safety_deposit_account_info = next_keyed_account(account_info_iter)?;
        let token_account_info = next_keyed_account(account_info_iter)?;
        let store_info = next_keyed_account(account_info_iter)?;
        let vault_info = next_keyed_account(account_info_iter)?;
        let vault_authority_info = next_keyed_account(account_info_iter)?;
        let payer_info = next_keyed_account(account_info_iter)?;
        let transfer_authority_info = next_keyed_account(account_info_iter)?;
        let rent = Rent::default();

        assert_owned_by(vault_info, program_id)?;
        assert_rent_exempt(&rent, token_account_info)?;
        assert_rent_exempt(&rent, vault_info)?;
        assert_owned_by(store_info, &mundis_token_program::id())?;

        if !safety_deposit_account_info.data_is_empty()? {
            return Err(VaultError::AlreadyInitialized.into());
        }

        let token_account: TokenAccount = assert_initialized(token_account_info)?;
        let store: TokenAccount = assert_initialized(store_info)?;
        let mut vault = Vault::from_keyed_account(vault_info)?;
        assert_vault_authority_correct(&vault, vault_authority_info)?;

        if vault.state != VaultState::Inactive {
            return Err(VaultError::VaultShouldBeInactive.into());
        }

        if token_account.amount == 0 {
            return Err(VaultError::TokenAccountContainsNoTokens.into());
        }

        if token_account.amount < amount {
            return Err(VaultError::TokenAccountAmountLessThanAmountSpecified.into());
        }

        if store.amount > 0 {
            return Err(VaultError::VaultAccountIsNotEmpty.into());
        }

        let seeds = &[
            PREFIX.as_bytes(),
            program_id.as_ref(),
            vault_info.unsigned_key().as_ref(),
        ];
        let (authority, _) = Pubkey::find_program_address(seeds, program_id);

        if store.owner != authority {
            return Err(VaultError::VaultAccountIsNotOwnedByProgram.into());
        }

        if store.delegate.is_some() {
            return Err(VaultError::DelegateShouldBeNone.into());
        }

        if store.close_authority.is_some() {
            return Err(VaultError::CloseAuthorityShouldBeNone.into());
        }

        let seeds = &[
            PREFIX.as_bytes(),
            vault_info.unsigned_key().as_ref(),
            token_account.mint.as_ref(),
        ];
        let (safety_deposit_account_key, bump_seed) = Pubkey::find_program_address(seeds, program_id);

        if safety_deposit_account_key != *safety_deposit_account_info.unsigned_key() {
            return Err(VaultError::SafetyDepositAddressInvalid.into());
        }

        let authority_signer_seeds = &[
            PREFIX.as_bytes(),
            vault_info.unsigned_key().as_ref(),
            token_account.mint.as_ref(),
            &[bump_seed],
        ];

        create_or_allocate_account_raw(
            *program_id,
            safety_deposit_account_info,
            &rent,
            payer_info,
            MAX_SAFETY_DEPOSIT_SIZE,
            authority_signer_seeds,
        )?;

        // let mut safety_deposit_account =
        //     SafetyDepositBox::from_account_info(safety_deposit_account_info)?;
        // safety_deposit_account.key = Key::SafetyDepositBoxV1;
        // safety_deposit_account.vault = *vault_info.unsigned_key();
        // safety_deposit_account.token_mint = token_account.mint;
        // safety_deposit_account.store = *store_info.unsigned_key();
        // safety_deposit_account.order = vault.token_type_count;
        //
        // safety_deposit_account.serialize(&mut *safety_deposit_account_info.try_account_ref()?.data_mut())?;

        vault.token_type_count = match vault.token_type_count.checked_add(1) {
            Some(val) => val,
            None => return Err(VaultError::NumericalOverflowError.into()),
        };

        vault.serialize(&mut vault_info.try_account_ref_mut()?.data_mut()).unwrap();

        // token_transfer(TokenTransferParams {
        //     source: token_account_info.clone(),
        //     destination: store_info.clone(),
        //     amount,
        //     authority: transfer_authority_info.clone(),
        //     authority_signer_seeds
        // }, invoke_context)?;

        Ok(())
    }

    pub fn process_init_vault(
        invoke_context: &mut InvokeContext,
        first_instruction_account: usize,
        allow_further_share_creation: bool,
    ) -> Result<(), InstructionError> {
        let keyed_accounts= invoke_context.get_keyed_accounts()?;

        let program_id = keyed_account_at_index(keyed_accounts, 0)?.unsigned_key();
        let accounts = &keyed_accounts[first_instruction_account..];

        let accounts_iter = &mut accounts.iter();
        let fraction_mint_info = next_keyed_account(accounts_iter)?;
        let redeem_treasury_info = next_keyed_account(accounts_iter)?;
        let fraction_treasury_info = next_keyed_account(accounts_iter)?;
        let vault_info = next_keyed_account(accounts_iter)?;
        let authority_info = next_keyed_account(accounts_iter)?;
        let pricing_lookup_address = next_keyed_account(accounts_iter)?;
        let rent = Rent::default();

        let fraction_mint: Mint = assert_initialized(fraction_mint_info)?;
        let redeem_treasury: TokenAccount = assert_initialized(redeem_treasury_info)?;
        let fraction_treasury: TokenAccount = assert_initialized(fraction_treasury_info)?;
        let mut vault = Vault::from_keyed_account(vault_info)?;

        if vault.key != Key::Uninitialized {
            return Err(VaultError::AlreadyInitialized.into());
        }

        let external_pricing_lookup = ExternalPriceAccount::from_keyed_account(pricing_lookup_address)?;

        assert_rent_exempt(&rent, redeem_treasury_info)?;
        assert_rent_exempt(&rent, fraction_treasury_info)?;
        assert_rent_exempt(&rent, fraction_mint_info)?;
        assert_rent_exempt(&rent, vault_info)?;
        assert_rent_exempt(&rent, pricing_lookup_address)?;
        assert_owned_by(fraction_mint_info, &mundis_token_program::id())?;
        assert_owned_by(fraction_treasury_info, &mundis_token_program::id())?;
        assert_owned_by(redeem_treasury_info, &mundis_token_program::id())?;
        assert_owned_by(vault_info, program_id)?;

        if fraction_mint.supply != 0 {
            return Err(VaultError::VaultMintNotEmpty.into());
        }

        let seeds = &[
            PREFIX.as_bytes(),
            program_id.as_ref(),
            vault_info.unsigned_key().as_ref(),
        ];
        let (authority, _) = Pubkey::find_program_address(seeds, program_id);

        match fraction_mint.mint_authority {
            None => {
                return Err(VaultError::VaultAuthorityNotProgram.into());
            }
            Some(val) => {
                if val != authority {
                    return Err(VaultError::VaultAuthorityNotProgram.into());
                }
            }
        }

        match fraction_mint.freeze_authority {
            None => {
                return Err(VaultError::VaultAuthorityNotProgram.into());
            }
            Some(val) => {
                if val != authority {
                    return Err(VaultError::VaultAuthorityNotProgram.into());
                }
            }
        }

        if redeem_treasury.amount != 0 {
            return Err(VaultError::TreasuryNotEmpty.into());
        }

        if redeem_treasury.owner != authority {
            return Err(VaultError::TreasuryOwnerNotProgram.into());
        }

        if redeem_treasury.delegate != None {
            return Err(VaultError::DelegateShouldBeNone.into());
        }

        if redeem_treasury.close_authority != None {
            return Err(VaultError::CloseAuthorityShouldBeNone.into());
        }

        if redeem_treasury.mint != external_pricing_lookup.price_mint {
            return Err(VaultError::RedeemTreasuryMintMustMatchLookupMint.into());
        }

        if redeem_treasury.mint == *fraction_mint_info.unsigned_key() {
            return Err(VaultError::RedeemTreasuryCantShareSameMintAsFraction.into());
        }

        if fraction_treasury.amount != 0 {
            return Err(VaultError::TreasuryNotEmpty.into());
        }

        if fraction_treasury.owner != authority {
            return Err(VaultError::TreasuryOwnerNotProgram.into());
        }

        if fraction_treasury.delegate != None {
            return Err(VaultError::DelegateShouldBeNone.into());
        }

        if fraction_treasury.close_authority != None {
            return Err(VaultError::CloseAuthorityShouldBeNone.into());
        }

        if fraction_treasury.mint != *fraction_mint_info.unsigned_key() {
            return Err(VaultError::VaultTreasuryMintDoesNotMatchVaultMint.into());
        }

        vault.key = Key::VaultV1;
        vault.redeem_treasury = *redeem_treasury_info.unsigned_key();
        vault.fraction_treasury = *fraction_treasury_info.unsigned_key();
        vault.fraction_mint = *fraction_mint_info.unsigned_key();
        vault.pricing_lookup_address = *pricing_lookup_address.unsigned_key();
        vault.allow_further_share_creation = allow_further_share_creation;
        vault.authority = *authority_info.unsigned_key();
        vault.token_type_count = 0;
        vault.state = VaultState::Inactive;

        vault.serialize(&mut vault_info.try_account_ref_mut()?.data_mut()).unwrap();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::BorrowMut;
    use std::cell::RefCell;
    use std::fmt::Error;
    use std::rc::Rc;
    use borsh::{BorshDeserialize, BorshSerialize};
    use mundis_program_runtime::invoke_context::mock_process_instruction;
    use mundis_sdk::account::{AccountSharedData, ReadableAccount, WritableAccount};
    use mundis_sdk::clock::Epoch;
    use mundis_sdk::decode_error::PrintInstructionError;
    use mundis_sdk::instruction::Instruction;
    use mundis_sdk::native_token::mdis_to_lamports;
    use mundis_sdk::program_pack::Pack;
    use mundis_sdk::rent::Rent;
    use mundis_token_program::state::{Mint, TokenAccount};
    use mundis_token_program::token_instruction::{initialize_account, initialize_account2, initialize_mint};
    use mundis_token_program::token_processor;
    use crate::processor::process_instruction;
    use crate::{InstructionError, Pubkey};
    use crate::error::VaultError;
    use crate::instruction::create_init_vault_instruction;
    use crate::state::{ExternalPriceAccount, Key, MAX_EXTERNAL_ACCOUNT_SIZE, MAX_VAULT_SIZE, PREFIX, VaultState};
    use crate::utils::try_from_slice_checked;

    fn process_vault_instruction(
        instruction: &Instruction,
        keyed_accounts: &[(bool, bool, Pubkey, Rc<RefCell<AccountSharedData>>)],
    ) -> Result<(), InstructionError> {
        mock_process_instruction(
            &mundis_sdk::token_vault::program::id(),
            Vec::new(),
            &instruction.data,
            keyed_accounts,
            super::process_instruction,
        )
    }

    fn process_token_instruction(
        instruction: &Instruction,
        keyed_accounts: &[(bool, bool, Pubkey, Rc<RefCell<AccountSharedData>>)],
    ) -> Result<(), InstructionError> {
        mock_process_instruction(
            &mundis_sdk::token::program::id(),
            Vec::new(),
            &instruction.data,
            keyed_accounts,
            token_processor::process_instruction,
        )
    }

    #[test]
    fn test_init_vault() {
        let token_program_id = mundis_sdk::token::program::id();
        let program_id = mundis_sdk::token_vault::program::id();
        let rent = Rent::default();

        let fraction_mint_key = Pubkey::new_unique();
        let fraction_mint_account = AccountSharedData::new_ref(
            rent.minimum_balance(Mint::get_packed_len()),
            Mint::get_packed_len(),
            &token_program_id
        );

        let redeem_treasury_key = Pubkey::new_unique();
        let redeem_treasury_account  = AccountSharedData::new_ref(
            rent.minimum_balance(TokenAccount::get_packed_len()),
            TokenAccount::get_packed_len(),
            &token_program_id
        );

        let fraction_treasury_key = Pubkey::new_unique();
        let fraction_treasury_account  = AccountSharedData::new_ref(
            rent.minimum_balance(TokenAccount::get_packed_len()),
            TokenAccount::get_packed_len(),
            &token_program_id
        );

        let vault_key = Pubkey::new_unique();
        let vault_account  = AccountSharedData::new_ref(
            rent.minimum_balance(MAX_VAULT_SIZE), MAX_VAULT_SIZE, &program_id
        );

        let vault_authority_key = Pubkey::new_unique();
        let vault_authority_account = AccountSharedData::new_ref(0, 0, &vault_authority_key);

        let seeds = &[PREFIX.as_bytes(), program_id.as_ref(), vault_key.as_ref()];
        let (authority, _) = Pubkey::find_program_address(seeds, &program_id);

        // create mint account for external price account
        let price_mint_key = Pubkey::new_unique();
        let price_mint_account = AccountSharedData::new_ref(
            rent.minimum_balance(Mint::LEN), Mint::LEN, &token_program_id
        );
        process_token_instruction(
            &initialize_mint(&token_program_id, &price_mint_key, &authority, Some(&authority), &"Token1".to_string(), &"TOK1".to_string(), 0).unwrap(),
            &[
                (true, true, price_mint_key, price_mint_account.clone()),
            ]
        ).unwrap();

        // create external account
        let external_price_account = ExternalPriceAccount {
            key: Key::ExternalAccountKeyV1,
            price_per_share: 0,
            price_mint: price_mint_key,
            allowed_to_combine: false
        };
        let external_price_account_key = Pubkey::new_unique();
        let mut account_data = Vec::<u8>::new();
        external_price_account.serialize(&mut account_data).unwrap();

        let external_price_account_account = AccountSharedData::create(
            rent.minimum_balance(MAX_EXTERNAL_ACCOUNT_SIZE), account_data, program_id, bool::default(), Epoch::default(),
        );
        let external_price_account_ref = Rc::new(RefCell::new(external_price_account_account));

        // initialize mint
        process_token_instruction(
            &initialize_mint(&token_program_id, &fraction_mint_key, &authority, Some(&authority), &"Token1".to_string(), &"TOK1".to_string(), 0).unwrap(),
            &[
                (true, true, fraction_mint_key, fraction_mint_account.clone()),
            ]
        ).unwrap();

        // initialize redeem_treasury token account
        process_token_instruction(
            &initialize_account2(&token_program_id, &redeem_treasury_key, &price_mint_key, &authority).unwrap(),
            &[
                (true, true, redeem_treasury_key, redeem_treasury_account.clone()),
                (true, true, price_mint_key, price_mint_account.clone()),
                (true, true, vault_authority_key, vault_authority_account.clone()),
            ]).unwrap();

        // initialize fraction_treasury token account
        process_token_instruction(
            &initialize_account2(&token_program_id, &fraction_treasury_key, &fraction_mint_key, &authority).unwrap(),
            &[
                (true, true, fraction_treasury_key, fraction_treasury_account.clone()),
                (true, true, fraction_mint_key, fraction_mint_account.clone()),
                (true, true, vault_authority_key, vault_authority_account.clone()),
            ]).unwrap();

        process_vault_instruction(
            &create_init_vault_instruction(&program_id, &fraction_mint_key, &redeem_treasury_key, &fraction_treasury_key, &vault_key, &vault_authority_key, &external_price_account_key, true).unwrap(),
            &[
                (false, true, fraction_mint_key, fraction_mint_account.clone()),
                (false, true, redeem_treasury_key, redeem_treasury_account.clone()),
                (false, true, fraction_treasury_key, fraction_treasury_account.clone()),
                (false, true, vault_key, vault_account.clone()),
                (false, true, vault_authority_key, vault_authority_account.clone()),
                (false, true, external_price_account_key, external_price_account_ref.clone()),
            ]
        ).unwrap();
    }
}