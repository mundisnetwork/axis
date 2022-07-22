use num_traits::FromPrimitive;
use mundis_program_runtime::ic_msg;

use mundis_program_runtime::invoke_context::InvokeContext;
use mundis_sdk::account::{ReadableAccount, WritableAccount};
use mundis_sdk::decode_error::DecodeError;
use mundis_sdk::keyed_account::{keyed_account_at_index, KeyedAccount, next_keyed_account};
use mundis_sdk::program_utils::limited_deserialize;
use mundis_sdk::instruction::InstructionError;
use mundis_sdk::program_memory::{sol_memcmp, sol_memset};
use mundis_sdk::program_pack::{IsInitialized, Pack};
use mundis_sdk::pubkey::Pubkey;
use mundis_sdk::rent::Rent;

use crate::{error::TokenError, state::{TokenAccount, AccountState, Mint, Multisig}, token_instruction::{AuthorityType, is_valid_signer_index, MAX_SIGNERS, TokenInstruction}};
use crate::error::PrintInstructionError;

pub fn process_instruction(
    first_instruction_account: usize,
    data: &[u8],
    invoke_context: &mut InvokeContext,
) -> Result<(), InstructionError> {
    if let Err(error) = Processor::process(first_instruction_account, data, invoke_context) {
        // catch the error so we can print it
        error.print::<TokenError>();
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
        let keyed_accounts = invoke_context.get_keyed_accounts()?;
        let program_id = keyed_account_at_index(keyed_accounts, 0)?.unsigned_key();
        let accounts = &keyed_accounts[first_instruction_account..];

        match limited_deserialize(data)? {
            TokenInstruction::InitializeMint {
                name,
                symbol,
                decimals,
                mint_authority,
                freeze_authority,
            } => {
                ic_msg!(invoke_context, "Instruction: InitializeMint");
                Self::process_initialize_mint(accounts, name, symbol, decimals, mint_authority, freeze_authority)
            }
            TokenInstruction::InitializeAccount => {
                ic_msg!(invoke_context, "Instruction: InitializeAccount");
                Self::process_initialize_account(program_id, accounts)
            }
            TokenInstruction::InitializeAccount2 { owner } => {
                ic_msg!(invoke_context, "Instruction: InitializeAccount2");
                Self::process_initialize_account2(program_id, accounts, owner)
            }
            TokenInstruction::InitializeMultisig { m } => {
                ic_msg!(invoke_context, "Instruction: InitializeMultisig");
                Self::process_initialize_multisig(accounts, m)
            }
            TokenInstruction::Transfer { amount } => {
                ic_msg!(invoke_context, "Instruction: Transfer");
                Self::process_transfer(program_id, accounts, amount, None)
            }
            TokenInstruction::Approve { amount } => {
                ic_msg!(invoke_context, "Instruction: Approve");
                Self::process_approve(program_id, accounts, amount, None)
            }
            TokenInstruction::Revoke => {
                ic_msg!(invoke_context, "Instruction: Revoke");
                Self::process_revoke(program_id, accounts)
            }
            TokenInstruction::SetAuthority {
                authority_type,
                new_authority,
            } => {
                ic_msg!(invoke_context, "Instruction: SetAuthority");
                Self::process_set_authority(program_id, accounts, authority_type, new_authority)
            }
            TokenInstruction::MintTo { amount } => {
                ic_msg!(invoke_context, "Instruction: MintTo");
                Self::process_mint_to(program_id, accounts, amount, None)
            }
            TokenInstruction::Burn { amount } => {
                ic_msg!(invoke_context, "Instruction: Burn");
                Self::process_burn(program_id, accounts, amount, None)
            }
            TokenInstruction::CloseAccount => {
                ic_msg!(invoke_context, "Instruction: CloseAccount");
                Self::process_close_account(program_id, accounts)
            }
            TokenInstruction::FreezeAccount => {
                println!("Instruction: FreezeAccount");
                Self::process_toggle_freeze_account(program_id, accounts, true)
            }
            TokenInstruction::ThawAccount => {
                ic_msg!(invoke_context, "Instruction: ThawAccount");
                Self::process_toggle_freeze_account(program_id, accounts, false)
            }
            TokenInstruction::TransferChecked { amount, decimals } => {
                ic_msg!(invoke_context, "Instruction: TransferChecked");
                Self::process_transfer(program_id, accounts, amount, Some(decimals))
            }
            TokenInstruction::ApproveChecked { amount, decimals } => {
                ic_msg!(invoke_context, "Instruction: ApproveChecked");
                Self::process_approve(program_id, accounts, amount, Some(decimals))
            }
            TokenInstruction::MintToChecked { amount, decimals } => {
                ic_msg!(invoke_context, "Instruction: MintToChecked");
                Self::process_mint_to(program_id, accounts, amount, Some(decimals))
            }
            TokenInstruction::BurnChecked { amount, decimals } => {
                ic_msg!(invoke_context, "Instruction: BurnChecked");
                Self::process_burn(program_id, accounts, amount, Some(decimals))
            }
            TokenInstruction::SyncNative => {
                ic_msg!(invoke_context, "Instruction: SyncNative");
                Self::process_sync_native(program_id, accounts)
            }
        }
    }

    pub fn process_initialize_mint(
        accounts: &[KeyedAccount],
        name: String,
        symbol: String,
        decimals: u8,
        mint_authority: Pubkey,
        freeze_authority: Option<Pubkey>,
    ) -> Result<(), InstructionError> {
        let accounts_iter = &mut accounts.iter();
        let mint_info = next_keyed_account(accounts_iter)?;
        let rent = Rent::default();

        let mut mint = Mint::unpack_unchecked(&mint_info.try_account_ref()?.data())?;
        if mint.is_initialized {
            return Err(TokenError::AlreadyInUse.into());
        }

        if !rent.is_exempt(mint_info.lamports()?, mint_info.data_len()?) {
            return Err(TokenError::NotRentExempt.into());
        }

        mint.mint_authority = Some(mint_authority);
        mint.decimals = decimals;
        mint.name = name;
        mint.symbol = symbol;
        mint.is_initialized = true;
        mint.freeze_authority = freeze_authority;

        Mint::pack(mint, mint_info.try_account_ref_mut()?.data_as_mut_slice())
    }

    fn _process_initialize_account(
        program_id: &Pubkey,
        accounts: &[KeyedAccount],
        owner: Option<&Pubkey>,
    ) -> Result<(), InstructionError> {
        let accounts_iter = &mut accounts.iter();
        let new_account_info = next_keyed_account(accounts_iter)?;
        let mint_info = next_keyed_account(accounts_iter)?;
        let owner = if let Some(owner) = owner {
            owner
        } else {
            next_keyed_account(accounts_iter)?.unsigned_key()
        };
        let rent = Rent::default();

        let mut token_account = TokenAccount::unpack_unchecked(new_account_info.try_account_ref()?.data())?;
        if token_account.is_initialized() {
            return Err(TokenError::AlreadyInUse.into());
        }

        if !rent.is_exempt(new_account_info.lamports()?, new_account_info.data_len()?) {
            return Err(TokenError::NotRentExempt.into());
        }

        let is_native_mint = Self::cmp_pubkeys(mint_info.unsigned_key(), &crate::native_mint::id());
        if !is_native_mint {
            Self::check_account_owner(program_id, mint_info)?;
            let _ = Mint::unpack(&mint_info.try_account_ref()?.data())
                .map_err(|_| Into::<InstructionError>::into(TokenError::InvalidMint))?;
        }

        token_account.mint = *mint_info.unsigned_key();
        token_account.owner = *owner;
        token_account.delegate = Option::None;
        token_account.delegated_amount = 0;
        token_account.state = AccountState::Initialized;

        if is_native_mint {
            let rent_exempt_reserve = rent.minimum_balance(new_account_info.data_len()?);
            token_account.is_native = Some(rent_exempt_reserve);
            token_account.amount = new_account_info.lamports()?
                .checked_sub(rent_exempt_reserve)
                .ok_or(TokenError::Overflow)?;
        } else {
            token_account.is_native = None;
            token_account.amount = 0;
        };

        TokenAccount::pack(token_account, new_account_info.try_account_ref_mut()?.data_as_mut_slice())
    }

    /// Processes an [InitializeAccount](enum.TokenInstruction.html) instruction.
    pub fn process_initialize_account(program_id: &Pubkey, accounts: &[KeyedAccount]) -> Result<(), InstructionError> {
        Self::_process_initialize_account(program_id, accounts, None)
    }

    /// Processes an [InitializeAccount2](enum.TokenInstruction.html) instruction.
    pub fn process_initialize_account2(program_id: &Pubkey, accounts: &[KeyedAccount], owner: Pubkey) -> Result<(), InstructionError> {
        Self::_process_initialize_account(program_id, accounts, Some(&owner))
    }

    /// Processes a [InitializeMultisig](enum.TokenInstruction.html) instruction.
    pub fn process_initialize_multisig(
        accounts: &[KeyedAccount],
        m: u8,
    ) -> Result<(), InstructionError> {
        let accounts_iter = &mut accounts.iter();
        let multisig_info = next_keyed_account(accounts_iter)?;

        let mut multisig = Multisig::unpack(multisig_info.try_account_ref()?.data())
            .unwrap_or(Multisig::default());
        if multisig.is_initialized {
            return Err(TokenError::AlreadyInUse.into());
        }

        let signer_infos = accounts_iter.as_slice();
        multisig.m = m;
        multisig.n = signer_infos.len() as u8;
        if !is_valid_signer_index(multisig.n as usize) {
            return Err(TokenError::InvalidNumberOfProvidedSigners.into());
        }
        if !is_valid_signer_index(multisig.m as usize) {
            return Err(TokenError::InvalidNumberOfRequiredSigners.into());
        }

        for (i, signer_info) in signer_infos.iter().enumerate() {
            multisig.signers[i] = *signer_info.unsigned_key();
        }

        multisig.is_initialized = true;

        Multisig::pack(multisig, multisig_info.try_account_ref_mut()?.data_as_mut_slice())
    }

    /// Processes a [Transfer](enum.TokenInstruction.html) instruction.
    pub fn process_transfer(
        program_id: &Pubkey,
        accounts: &[KeyedAccount],
        amount: u64,
        expected_decimals: Option<u8>,
    ) -> Result<(), InstructionError> {
        let account_info_iter = &mut accounts.iter();
        let source_account_info = next_keyed_account(account_info_iter)?;

        let expected_mint_info = if let Some(expected_decimals) = expected_decimals {
            Some((next_keyed_account(account_info_iter)?, expected_decimals))
        } else {
            None
        };

        let dest_account_info = next_keyed_account(account_info_iter)?;
        let authority_info = next_keyed_account(account_info_iter)?;

        let mut source_account = TokenAccount::unpack(source_account_info.try_account_ref()?.data())?;
        let mut dest_account = TokenAccount::unpack(dest_account_info.try_account_ref()?.data())?;

        if source_account.is_frozen() || dest_account.is_frozen() {
            return Err(TokenError::AccountFrozen.into());
        }
        if source_account.amount < amount {
            return Err(TokenError::InsufficientFunds.into());
        }
        if !Self::cmp_pubkeys(&source_account.mint, &dest_account.mint) {
            return Err(TokenError::MintMismatch.into());
        }

        if let Some((mint_info, expected_decimals)) = expected_mint_info {
            if !Self::cmp_pubkeys(mint_info.unsigned_key(), &source_account.mint) {
                return Err(TokenError::MintMismatch.into());
            }

            let mint = Mint::unpack(&mint_info.try_account_ref()?.data())?;
            if expected_decimals != mint.decimals {
                return Err(TokenError::MintDecimalsMismatch.into());
            }
        }

        let self_transfer = Self::cmp_pubkeys(source_account_info.unsigned_key(), dest_account_info.unsigned_key());

        match source_account.delegate {
            Some(ref delegate) if Self::cmp_pubkeys(authority_info.unsigned_key(),delegate) => {
                Self::validate_owner(
                    program_id,
                    delegate,
                    authority_info,
                    account_info_iter.as_slice(),
                )?;
                if source_account.delegated_amount < amount {
                    return Err(TokenError::InsufficientFunds.into());
                }
                if !self_transfer {
                    source_account.delegated_amount = source_account
                        .delegated_amount
                        .checked_sub(amount)
                        .ok_or(TokenError::Overflow)?;
                    if source_account.delegated_amount == 0 {
                        source_account.delegate = None;
                    }
                }
            }
            _ => Self::validate_owner(
                program_id,
                &source_account.owner,
                authority_info,
                account_info_iter.as_slice(),
            )?,
        }

        if self_transfer || amount == 0 {
            Self::check_account_owner(program_id, source_account_info)?;
            Self::check_account_owner(program_id, dest_account_info)?;
        }

        // This check MUST occur just before the amounts are manipulated
        // to ensure self-transfers are fully validated
        if self_transfer {
            return Ok(());
        }

        source_account.amount = source_account
            .amount
            .checked_sub(amount)
            .ok_or(TokenError::Overflow)?;
        dest_account.amount = dest_account
            .amount
            .checked_add(amount)
            .ok_or(TokenError::Overflow)?;

        if source_account.is_native() {
            let source_starting_lamports = source_account_info.lamports()?;
            source_account_info.try_account_ref_mut()?.set_lamports(
                source_starting_lamports
                    .checked_sub(amount)
                    .ok_or(TokenError::Overflow)?
            );

            let dest_starting_lamports = dest_account_info.lamports()?;
            dest_account_info.try_account_ref_mut()?.set_lamports(dest_starting_lamports
                .checked_add(amount)
                .ok_or(TokenError::Overflow)?
            );
        }

        TokenAccount::pack(source_account, source_account_info.try_account_ref_mut()?.data_as_mut_slice())
            .and_then(|_| TokenAccount::pack(dest_account, dest_account_info.try_account_ref_mut()?.data_as_mut_slice()))
    }

    /// Processes an [Approve](enum.TokenInstruction.html) instruction.
    pub fn process_approve(
        program_id: &Pubkey,
        accounts: &[KeyedAccount],
        amount: u64,
        expected_decimals: Option<u8>,
    ) -> Result<(), InstructionError> {
        let account_info_iter = &mut accounts.iter();

        let source_account_info = next_keyed_account(account_info_iter)?;

        let expected_mint_info = if let Some(expected_decimals) = expected_decimals {
            Some((next_keyed_account(account_info_iter)?, expected_decimals))
        } else {
            None
        };

        let delegate_info = next_keyed_account(account_info_iter)?;
        let owner_info = next_keyed_account(account_info_iter)?;

        let mut source_account = TokenAccount::unpack(source_account_info.try_account_ref()?.data())?;

        if source_account.is_frozen() {
            return Err(TokenError::AccountFrozen.into());
        }

        if let Some((mint_info, expected_decimals)) = expected_mint_info {
            if !Self::cmp_pubkeys(mint_info.unsigned_key(), &source_account.mint) {
                return Err(TokenError::MintMismatch.into());
            }

            let mint = Mint::unpack(&mint_info.try_account_ref()?.data())?;
            if expected_decimals != mint.decimals {
                return Err(TokenError::MintDecimalsMismatch.into());
            }
        }

        Self::validate_owner(
            program_id,
            &source_account.owner,
            owner_info,
            account_info_iter.as_slice(),
        )?;

        source_account.delegate = Some(*delegate_info.unsigned_key());
        source_account.delegated_amount = amount;

        TokenAccount::pack(source_account, source_account_info.try_account_ref_mut()?.data_as_mut_slice())
    }

    /// Processes an [Revoke](enum.TokenInstruction.html) instruction.
    pub fn process_revoke(
        program_id: &Pubkey,
        accounts: &[KeyedAccount],
    ) -> Result<(), InstructionError> {
        let account_info_iter = &mut accounts.iter();
        let source_account_info = next_keyed_account(account_info_iter)?;

        let mut source_account = TokenAccount::unpack(source_account_info.try_account_ref()?.data())?;

        let owner_info = next_keyed_account(account_info_iter)?;

        if source_account.is_frozen() {
            return Err(TokenError::AccountFrozen.into());
        }

        Self::validate_owner(
            program_id,
            &source_account.owner,
            owner_info,
            account_info_iter.as_slice(),
        )?;

        source_account.delegate = None;
        source_account.delegated_amount = 0;

        TokenAccount::pack(source_account, source_account_info.try_account_ref_mut()?.data_as_mut_slice())
    }

    pub fn process_set_authority(
        program_id: &Pubkey,
        accounts: &[KeyedAccount],
        authority_type: AuthorityType,
        new_authority: Option<Pubkey>,
    ) -> Result<(), InstructionError> {
        let account_info_iter = &mut accounts.iter();
        let account_info = next_keyed_account(account_info_iter)?;
        let authority_info = next_keyed_account(account_info_iter)?;

        if account_info.data_len()? == TokenAccount::get_packed_len() {
            let mut account = TokenAccount::unpack(account_info.try_account_ref()?.data())?;

            if account.is_frozen() {
                return Err(TokenError::AccountFrozen.into());
            }

            match authority_type {
                AuthorityType::AccountOwner => {
                    Self::validate_owner(
                        program_id,
                        &account.owner,
                        authority_info,
                        account_info_iter.as_slice(),
                    )?;

                    if let Some(authority) = new_authority {
                        account.owner = authority;
                    } else {
                        return Err(TokenError::InvalidInstruction.into());
                    }

                    account.delegate = None;
                    account.delegated_amount = 0;

                    if account.is_native() {
                        account.close_authority = None;
                    }
                }
                AuthorityType::CloseAccount => {
                    let authority = account.close_authority.unwrap_or(account.owner);
                    Self::validate_owner(
                        program_id,
                        &authority,
                        authority_info,
                        account_info_iter.as_slice(),
                    )?;
                    account.close_authority = new_authority;
                }
                _ => {
                    return Err(TokenError::AuthorityTypeNotSupported.into());
                }
            }
            TokenAccount::pack(account, account_info.try_account_ref_mut()?.data_as_mut_slice())
        } else if account_info.data_len()? == Mint::get_packed_len() {
            let mut mint = Mint::unpack(account_info.try_account_ref()?.data())?;
            match authority_type {
                AuthorityType::MintTokens => {
                    // Once a mint's supply is fixed, it cannot be undone by setting a new
                    // mint_authority
                    let mint_authority = mint
                        .mint_authority
                        .ok_or(Into::<InstructionError>::into(TokenError::FixedSupply))?;
                    Self::validate_owner(
                        program_id,
                        &mint_authority,
                        authority_info,
                        account_info_iter.as_slice(),
                    )?;
                    mint.mint_authority = new_authority;
                }
                AuthorityType::FreezeAccount => {
                    // Once a mint's freeze authority is disabled, it cannot be re-enabled by
                    // setting a new freeze_authority
                    let freeze_authority = mint
                        .freeze_authority
                        .ok_or(Into::<InstructionError>::into(TokenError::MintCannotFreeze))?;
                    Self::validate_owner(
                        program_id,
                        &freeze_authority,
                        authority_info,
                        account_info_iter.as_slice(),
                    )?;
                    mint.freeze_authority = new_authority;
                }
                _ => {
                    return Err(TokenError::AuthorityTypeNotSupported.into());
                }
            }
            Mint::pack(mint, account_info.try_account_ref_mut()?.data_as_mut_slice())
        } else {
            return Err(InstructionError::InvalidArgument);
        }
    }

    /// Processes a [MintTo](enum.TokenInstruction.html) instruction.
    pub fn process_mint_to(
        program_id: &Pubkey,
        accounts: &[KeyedAccount],
        amount: u64,
        expected_decimals: Option<u8>,
    ) -> Result<(), InstructionError> {
        let account_info_iter = &mut accounts.iter();
        let mint_info = next_keyed_account(account_info_iter)?;
        let dest_account_info = next_keyed_account(account_info_iter)?;
        let owner_info = next_keyed_account(account_info_iter)?;

        let mut dest_account = TokenAccount::unpack(dest_account_info.try_account_ref()?.data())?;

        if dest_account.is_frozen() {
            return Err(TokenError::AccountFrozen.into());
        }

        if dest_account.is_native() {
            return Err(TokenError::NativeNotSupported.into());
        }
        if mint_info.unsigned_key() != &dest_account.mint {
            return Err(TokenError::MintMismatch.into());
        }

        let mut mint = Mint::unpack(mint_info.try_account_ref()?.data())?;
        if let Some(expected_decimals) = expected_decimals {
            if expected_decimals != mint.decimals {
                return Err(TokenError::MintDecimalsMismatch.into());
            }
        }

        match mint.mint_authority {
            Some(mint_authority) => Self::validate_owner(
                program_id,
                &mint_authority,
                owner_info,
                account_info_iter.as_slice(),
            )?,
            None => return Err(TokenError::FixedSupply.into()),
        }

        if amount == 0 {
            Self::check_account_owner(program_id, mint_info)?;
            Self::check_account_owner(program_id, dest_account_info)?;
        }

        dest_account.amount = dest_account
            .amount
            .checked_add(amount)
            .ok_or(TokenError::Overflow)?;

        mint.supply = mint
            .supply
            .checked_add(amount)
            .ok_or(TokenError::Overflow)?;

        TokenAccount::pack(dest_account, dest_account_info.try_account_ref_mut()?.data_as_mut_slice())
            .and_then(|_| Mint::pack(mint, mint_info.try_account_ref_mut()?.data_as_mut_slice()))
    }

    /// Processes a [Burn](enum.TokenInstruction.html) instruction.
    pub fn process_burn(
        program_id: &Pubkey,
        accounts: &[KeyedAccount],
        amount: u64,
        expected_decimals: Option<u8>,
    ) -> Result<(), InstructionError> {
        let account_info_iter = &mut accounts.iter();

        let source_account_info = next_keyed_account(account_info_iter)?;
        let mint_info = next_keyed_account(account_info_iter)?;
        let authority_info = next_keyed_account(account_info_iter)?;

        let mut source_account = TokenAccount::unpack(source_account_info.try_account_ref()?.data())?;

        let mut mint = Mint::unpack(mint_info.try_account_ref()?.data())?;

        if source_account.is_frozen() {
            return Err(TokenError::AccountFrozen.into());
        }
        if source_account.is_native() {
            return Err(TokenError::NativeNotSupported.into());
        }
        if source_account.amount < amount {
            return Err(TokenError::InsufficientFunds.into());
        }
        if !Self::cmp_pubkeys(mint_info.unsigned_key(), &source_account.mint) {
            return Err(TokenError::MintMismatch.into());
        }

        if let Some(expected_decimals) = expected_decimals {
            if expected_decimals != mint.decimals {
                return Err(TokenError::MintDecimalsMismatch.into());
            }
        }

        match source_account.delegate {
            Some(ref delegate) if Self::cmp_pubkeys(authority_info.unsigned_key(), delegate) => {
                Self::validate_owner(
                    program_id,
                    delegate,
                    authority_info,
                    account_info_iter.as_slice(),
                )?;

                if source_account.delegated_amount < amount {
                    return Err(TokenError::InsufficientFunds.into());
                }
                source_account.delegated_amount = source_account
                    .delegated_amount
                    .checked_sub(amount)
                    .ok_or(TokenError::Overflow)?;
                if source_account.delegated_amount == 0 {
                    source_account.delegate = None;
                }
            }
            _ => Self::validate_owner(
                program_id,
                &source_account.owner,
                authority_info,
                account_info_iter.as_slice(),
            )?,
        }

        if amount == 0 {
            Self::check_account_owner(program_id, source_account_info)?;
            Self::check_account_owner(program_id, mint_info)?;
        }

        source_account.amount = source_account
            .amount
            .checked_sub(amount)
            .ok_or(TokenError::Overflow)?;
        mint.supply = mint
            .supply
            .checked_sub(amount)
            .ok_or(TokenError::Overflow)?;

        TokenAccount::pack(source_account, source_account_info.try_account_ref_mut()?.data_as_mut_slice())
            .and_then(|_| Mint::pack(mint, mint_info.try_account_ref_mut()?.data_as_mut_slice()))
    }

    /// Processes a [CloseAccount](enum.TokenInstruction.html) instruction.
    pub fn process_close_account(program_id: &Pubkey, accounts: &[KeyedAccount]) -> Result<(), InstructionError> {
        let account_info_iter = &mut accounts.iter();
        let source_account_info = next_keyed_account(account_info_iter)?;
        let dest_account_info = next_keyed_account(account_info_iter)?;
        let authority_info = next_keyed_account(account_info_iter)?;

        if Self::cmp_pubkeys(source_account_info.unsigned_key(), dest_account_info.unsigned_key()) {
            return Err(InstructionError::InvalidAccountData);
        }

        let source_account = TokenAccount::unpack(source_account_info.try_account_ref()?.data())?;
        if !source_account.is_native() && source_account.amount != 0 {
            return Err(TokenError::NonNativeHasBalance.into());
        }

        let authority = source_account
            .close_authority
            .unwrap_or(source_account.owner);
        Self::validate_owner(
            program_id,
            &authority,
            authority_info,
            account_info_iter.as_slice(),
        )?;

        let dest_starting_lamports = dest_account_info.lamports()?;
        dest_account_info.try_account_ref_mut()?.set_lamports(
            dest_starting_lamports
                .checked_add(source_account_info.lamports()?)
                .ok_or(TokenError::Overflow)?
        );

        source_account_info.try_account_ref_mut()?.set_lamports(0);

        sol_memset(source_account_info.try_account_ref_mut()?.data_mut(), 0, TokenAccount::LEN);

        Ok(())
    }

    /// Processes a [FreezeAccount](enum.TokenInstruction.html) or a
    /// [ThawAccount](enum.TokenInstruction.html) instruction.
    pub fn process_toggle_freeze_account(
        program_id: &Pubkey,
        accounts: &[KeyedAccount],
        freeze: bool,
    ) -> Result<(), InstructionError> {
        let account_info_iter = &mut accounts.iter();
        let source_account_info = next_keyed_account(account_info_iter)?;
        let mint_info = next_keyed_account(account_info_iter)?;
        let authority_info = next_keyed_account(account_info_iter)?;

        let mut source_account = TokenAccount::unpack(source_account_info.try_account_ref()?.data())?;
        if freeze && source_account.is_frozen() || !freeze && !source_account.is_frozen() {
            return Err(TokenError::InvalidState.into());
        }
        if source_account.is_native() {
            return Err(TokenError::NativeNotSupported.into());
        }
        if !Self::cmp_pubkeys(mint_info.unsigned_key(), &source_account.mint) {
            return Err(TokenError::MintMismatch.into());
        }

        let mint = Mint::unpack(mint_info.try_account_ref()?.data())?;
        match mint.freeze_authority {
            Some(authority) => Self::validate_owner(
                program_id,
                &authority,
                authority_info,
                account_info_iter.as_slice(),
            ),
            None => Err(TokenError::MintCannotFreeze.into()),
        }?;

        source_account.state = if freeze {
            AccountState::Frozen
        } else {
            AccountState::Initialized
        };

        TokenAccount::pack(source_account, source_account_info.try_account_ref_mut()?.data_as_mut_slice())
    }

    /// Processes a [SyncNative](enum.TokenInstruction.html) instruction
    pub fn process_sync_native(program_id: &Pubkey, accounts: &[KeyedAccount]) -> Result<(), InstructionError> {
        let account_info_iter = &mut accounts.iter();
        let native_account_info = next_keyed_account(account_info_iter)?;
        Self::check_account_owner(program_id, native_account_info)?;

        let mut native_account = TokenAccount::unpack(native_account_info.try_account_ref()?.data())?;

        if let Some(rent_exempt_reserve) = native_account.is_native  {
            let new_amount = native_account_info
                .lamports()?
                .checked_sub(rent_exempt_reserve)
                .ok_or(TokenError::Overflow)?;
            if new_amount < native_account.amount {
                return Err(TokenError::InvalidState.into());
            }
            native_account.amount = new_amount;
        }  else {
            return Err(TokenError::NonNativeNotSupported.into());
        }

        TokenAccount::pack(native_account, native_account_info.try_account_ref_mut()?.data_as_mut_slice())
    }

    /// Checks that the account is owned by the expected program
    pub fn check_account_owner(program_id: &Pubkey, account_info: &KeyedAccount) -> Result<(), InstructionError> {
        if !Self::cmp_pubkeys(program_id, &account_info.owner()?) {
            Err(InstructionError::IncorrectProgramId)
        } else {
            Ok(())
        }
    }

    /// Checks two pubkeys for equality in a computationally cheap way using
    /// `sol_memcmp`
    pub fn cmp_pubkeys(a: &Pubkey, b: &Pubkey) -> bool {
        sol_memcmp(a.as_ref(), b.as_ref(), mundis_sdk::pubkey::PUBKEY_BYTES) == 0
    }

    /// Validates owner(s) are present
    pub fn validate_owner(
        program_id: &Pubkey,
        expected_owner: &Pubkey,
        owner_account_info: &KeyedAccount,
        signers: &[KeyedAccount],
    ) -> Result<(), InstructionError> {
        if !Self::cmp_pubkeys(expected_owner, owner_account_info.unsigned_key()) {
            return Err(TokenError::OwnerMismatch.into());
        }
        if Self::cmp_pubkeys(program_id, &owner_account_info.owner()?)
            && owner_account_info.data_len()? == Multisig::get_packed_len()
        {
            let multisig = Multisig::unpack(owner_account_info.try_account_ref()?.data())?;
            let mut num_signers = 0;
            let mut matched = [false; MAX_SIGNERS];
            for signer in signers.iter() {
                for (position, key) in multisig.signers[0..multisig.n as usize].iter().enumerate() {
                    if Self::cmp_pubkeys(key, signer.unsigned_key()) && !matched[position] {
                        if signer.signer_key().is_none() {
                            return Err(InstructionError::MissingRequiredSignature);
                        }
                        matched[position] = true;
                        num_signers += 1;
                    }
                }
            }
            if num_signers < multisig.m {
                return Err(InstructionError::MissingRequiredSignature);
            }
            return Ok(());
        } else if owner_account_info.signer_key().is_none() {
            return Err(InstructionError::MissingRequiredSignature);
        }
        Ok(())
    }
}

impl PrintInstructionError for TokenError {
    fn print<E>(&self)
        where
            E: 'static + std::error::Error + DecodeError<E> + PrintInstructionError + FromPrimitive,
    {
        match self {
            TokenError::NotRentExempt => eprintln!("Error: lamport balance below rent-exempt threshold"),
            TokenError::InsufficientFunds => eprintln!("Error: insufficient funds"),
            TokenError::InvalidMint => eprintln!("Error: Invalid Mint"),
            TokenError::MintMismatch => eprintln!("Error: Account not associated with this Mint"),
            TokenError::OwnerMismatch => eprintln!("Error: owner does not match"),
            TokenError::FixedSupply => eprintln!("Error: the total supply of this token is fixed"),
            TokenError::AlreadyInUse => eprintln!("Error: account or token already in use"),
            TokenError::InvalidNumberOfProvidedSigners => {
                eprintln!("Error: Invalid number of provided signers")
            }
            TokenError::InvalidNumberOfRequiredSigners => {
                eprintln!("Error: Invalid number of required signers")
            }
            TokenError::UninitializedState => eprintln!("Error: State is uninitialized"),
            TokenError::NativeNotSupported => {
                eprintln!("Error: Instruction does not support native tokens")
            }
            TokenError::NonNativeHasBalance => {
                eprintln!("Error: Non-native account can only be closed if its balance is zero")
            }
            TokenError::InvalidInstruction => eprintln!("Error: Invalid instruction"),
            TokenError::InvalidState => eprintln!("Error: Invalid account state for operation"),
            TokenError::Overflow => eprintln!("Error: Operation overflowed"),
            TokenError::AuthorityTypeNotSupported => {
                eprintln!("Error: Account does not support specified authority type")
            }
            TokenError::MintCannotFreeze => eprintln!("Error: This token mint cannot freeze accounts"),
            TokenError::AccountFrozen => eprintln!("Error: Account is frozen"),
            TokenError::MintDecimalsMismatch => {
                eprintln!("Error: decimals different from the Mint decimals")
            }
            TokenError::NonNativeNotSupported => {
                eprintln!("Error: Instruction does not support non-native tokens")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use mundis_program_runtime::invoke_context::mock_process_instruction;
    use mundis_sdk::account::{AccountSharedData, ReadableAccount, WritableAccount};
    use mundis_sdk::instruction::{Instruction, InstructionError};
    use mundis_sdk::program_pack::Pack;
    use mundis_sdk::pubkey::Pubkey;
    use mundis_sdk::rent::Rent;

    use crate::error::{PrintInstructionError, TokenError};
    use crate::state::{TokenAccount, Mint, Multisig, puffed_out_string, MAX_NAME_LENGTH, MAX_SYMBOL_LENGTH};
    use crate::token_instruction::*;

    fn process_token_instruction(
        instruction: &Instruction,
        keyed_accounts: &[(bool, bool, Pubkey, Rc<RefCell<AccountSharedData>>)],
    ) -> Result<(), InstructionError> {
        mock_process_instruction(
            &mundis_sdk::token::program::id(),
            Vec::new(),
            &instruction.data,
            keyed_accounts,
            super::process_instruction,
        )
    }

    fn return_token_error_as_program_error() -> InstructionError {
        TokenError::MintMismatch.into()
    }

    #[test]
    fn test_print_error() {
        let error = return_token_error_as_program_error();
        error.print::<TokenError>();
    }

    #[test]
    #[should_panic(expected = "Custom(3)")]
    fn test_error_unwrap() {
        Err::<(), InstructionError>(return_token_error_as_program_error()).unwrap();
    }

    #[test]
    fn test_unique_account_sizes() {
        assert_ne!(Mint::get_packed_len(), 0);
        assert_ne!(Mint::get_packed_len(), TokenAccount::get_packed_len());
        assert_ne!(Mint::get_packed_len(), Multisig::get_packed_len());
        assert_ne!(TokenAccount::get_packed_len(), 0);
        assert_ne!(TokenAccount::get_packed_len(), Multisig::get_packed_len());
        assert_ne!(Multisig::get_packed_len(), 0);
    }

    #[test]
    fn test_initialize_mint() {
        let program_id = mundis_sdk::token::program::id();
        let owner_key = Pubkey::new_unique();
        let mint_key = Pubkey::new_unique();
        let rent = Rent::default();
        let mint_account = AccountSharedData::new_ref(
            rent.minimum_balance(Mint::get_packed_len()),
            Mint::get_packed_len(),
            &program_id
        );
        let name = String::from("Test Token");
        let symbol = String::from("TST");

        // create new mint
        process_token_instruction(
            &initialize_mint(&program_id, &mint_key, &owner_key, None, &name, &symbol, 2).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
            ]).unwrap();

        // create twice
        assert_eq!(
            Err(TokenError::AlreadyInUse.into()),
            process_token_instruction(
                &initialize_mint(&program_id, &mint_key, &owner_key, None, &name, &symbol, 2).unwrap(),
                &[
                    (true, true, mint_key, mint_account.clone()),
                ])
        );
    }

    #[test]
    fn test_initialize_mint_account() {
        let program_id = mundis_sdk::token::program::id();
        let rent = Rent::default();
        let account_key = Pubkey::new_unique();
        let account_account = AccountSharedData::new_ref(rent.minimum_balance(TokenAccount::get_packed_len()), TokenAccount::get_packed_len(), &program_id);
        let owner_key = Pubkey::new_unique();
        let owner_account = AccountSharedData::new_ref(0, 0, &owner_key);
        let mint_key = Pubkey::new_unique();
        let mint_account = AccountSharedData::new_ref(rent.minimum_balance(Mint::get_packed_len()), Mint::get_packed_len(), &program_id);
        let name = String::from("Test Token");
        let symbol = String::from("TST");

        // mint is not valid (not initialized)
        assert_eq!(
            Err(TokenError::InvalidMint.into()),
            process_token_instruction(
                &initialize_account(&program_id, &account_key, &mint_key, &owner_key).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, mint_key, mint_account.clone()),
                    (true, true, owner_key, owner_account.clone()),
                ])
        );

        // create mint
        process_token_instruction(
            &initialize_mint(&program_id, &mint_key, &owner_key, None, &name, &symbol, 2).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
            ]).unwrap();

        // create account
        process_token_instruction(
            &initialize_account(&program_id, &account_key, &mint_key, &owner_key).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ]).unwrap();

        // create twice
        assert_eq!(
            Err(TokenError::AlreadyInUse.into()),
            process_token_instruction(
                &initialize_account(&program_id, &account_key, &mint_key, &owner_key).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, mint_key, mint_account.clone()),
                    (true, true, owner_key, owner_account.clone()),
                ])
        );
    }

    #[test]
    fn test_transfer_dups() {
        let program_id = mundis_sdk::token::program::id();
        let rent = Rent::default();
        let account1_key = Pubkey::new_unique();
        let account1_account = AccountSharedData::new_ref(rent.minimum_balance(TokenAccount::get_packed_len()), TokenAccount::get_packed_len(), &program_id);
        let account2_key = Pubkey::new_unique();
        let account2_account = AccountSharedData::new_ref(rent.minimum_balance(TokenAccount::get_packed_len()), TokenAccount::get_packed_len(), &program_id);
        let account3_key = Pubkey::new_unique();
        let account3_account = AccountSharedData::new_ref(rent.minimum_balance(TokenAccount::get_packed_len()), TokenAccount::get_packed_len(), &program_id);
        let account4_key = Pubkey::new_unique();
        let account4_account = AccountSharedData::new_ref(rent.minimum_balance(TokenAccount::get_packed_len()), TokenAccount::get_packed_len(), &program_id);
        let owner_key = Pubkey::new_unique();
        let owner_account = AccountSharedData::new_ref(rent.minimum_balance(TokenAccount::get_packed_len()), TokenAccount::get_packed_len(), &owner_key);
        let mint_key = Pubkey::new_unique();
        let mint_account = AccountSharedData::new_ref(rent.minimum_balance(Mint::get_packed_len()), Mint::get_packed_len(), &program_id);
        let multisig_key = Pubkey::new_unique();
        let multisig_account = AccountSharedData::new_ref(0, Multisig::get_packed_len(), &program_id);

        // create mint
        process_token_instruction(
            &initialize_mint(&program_id,&mint_key, &owner_key, None, &"Test Token".to_string(), &"TST".to_string(), 2).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
            ],
        ).unwrap();

        // create account
        process_token_instruction(
            &initialize_account(&program_id,&account1_key, &mint_key, &account1_key).unwrap(),
            &[
                (true, true, account1_key, account1_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, account1_key, account1_account.clone()),
            ],
        ).unwrap();

        // create another account
        process_token_instruction(
            &initialize_account( &program_id,&account2_key, &mint_key, &owner_key).unwrap(),
            &[
                (true, true, account2_key, account2_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // mint to account
        process_token_instruction(
            &mint_to(&program_id,&mint_key, &account1_key, &owner_key, &[], 1000).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
                (true, true, account1_key, account1_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // source-owner transfer
        process_token_instruction(
            &transfer(&program_id,&account1_key, &account2_key, &account1_key, &[], 500,).unwrap(),
            &[
                (true, true, account1_key, account1_account.clone()),
                (true, true, account2_key, account2_account.clone()),
                (true, true, account1_key, account1_account.clone()),
            ],
        ).unwrap();

        // source-delegate transfer
        let mut account = TokenAccount::unpack(account1_account.try_borrow().unwrap().data()).unwrap();
        account.amount = 1000;
        account.delegated_amount = 1000;
        account.delegate = Some(account1_key);
        account.owner = owner_key;
        TokenAccount::pack(account, account1_account.try_borrow_mut().unwrap().data_as_mut_slice()).unwrap();
        process_token_instruction(
            &transfer(&program_id,&account1_key, &account2_key, &account1_key, &[], 500,).unwrap(),
            &[
                (true, true, account1_key, account1_account.clone()),
                (true, true, account2_key, account2_account.clone()),
                (true, true, account1_key, account1_account.clone()),
            ],
        ).unwrap();

        // source-delegate TransferChecked
        process_token_instruction(
            &transfer_checked(
                &program_id,
                &account1_key,
                &mint_key,
                &account2_key,
                &account1_key,
                &[],
                500,
                2,
            ).unwrap(),
            &[
                (true, true, account1_key, account1_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, account2_key, account2_account.clone()),
                (true, true, account1_key, account1_account.clone()),
            ],
        ).unwrap();

        // test destination-owner transfer
        process_token_instruction(
            &initialize_account(&program_id,&account3_key, &mint_key, &account2_key).unwrap(),
            &[
                (true, true, account3_key, account3_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, account2_key, account2_account.clone()),
            ],
        ).unwrap();
        process_token_instruction(
            &mint_to(&program_id,&mint_key, &account3_key, &owner_key, &[], 1000).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
                (true, true, account3_key, account3_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();
        process_token_instruction(
            &transfer(&program_id,&account3_key, &account2_key, &account2_key, &[], 500,).unwrap(),
            &[
                (true, true, account3_key, account3_account.clone()),
                (true, true, account2_key, account2_account.clone()),
                (true, true, account2_key, account2_account.clone()),
            ],
        ).unwrap();

        // destination-owner TransferChecked
        process_token_instruction(
            &transfer_checked(
                &program_id,
                &account3_key,
                &mint_key,
                &account2_key,
                &account2_key,
                &[],
                500,
                2,
            ).unwrap(),
            &[
                (true, true, account3_key, account3_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, account2_key, account2_account.clone()),
                (true, true, account2_key, account2_account.clone()),
            ],
        ).unwrap();

        // test source-multisig signer
        process_token_instruction(
            &initialize_multisig(&program_id,&multisig_key, &[&account4_key], 1).unwrap(),
            &[
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, account4_key, account4_account.clone())
            ],
        ).unwrap();
        process_token_instruction(
            &initialize_account(&program_id,&account4_key, &mint_key, &multisig_key).unwrap(),
            &[
                (true, true, account4_key, account4_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
            ],
        ).unwrap();
        process_token_instruction(
            &mint_to(&program_id,&mint_key, &account4_key, &owner_key, &[], 1000).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
                (true, true, account4_key, account4_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // source-multisig-signer transfer
        process_token_instruction(
            &transfer(&program_id,&account4_key, &account2_key, &multisig_key, &[&account4_key], 500,).unwrap(),
            &[
                (true, true, account4_key, account4_account.clone()),
                (true, true, account2_key, account2_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, account4_key, account4_account.clone()),
            ],
        ).unwrap();

        // source-multisig-signer TransferChecked
        process_token_instruction(
            &transfer_checked(
                &program_id,
                &account4_key,
                &mint_key,
                &account2_key,
                &multisig_key,
                &[&account4_key],
                500,
                2,
            ).unwrap(),
            &[
                (true, true, account4_key, account4_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, account2_key, account2_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, account4_key, account4_account.clone()),
            ],
        ).unwrap();
    }

    #[test]
    fn test_transfer() {
        let program_id = mundis_sdk::token::program::id();
        let rent = Rent::default();
        let account_key = Pubkey::new_unique();
        let account_account = AccountSharedData::new_ref(rent.minimum_balance(TokenAccount::get_packed_len()), TokenAccount::get_packed_len(), &program_id);
        let account2_key = Pubkey::new_unique();
        let account2_account = AccountSharedData::new_ref(rent.minimum_balance(TokenAccount::get_packed_len()), TokenAccount::get_packed_len(), &program_id);
        let account3_key = Pubkey::new_unique();
        let account3_account = AccountSharedData::new_ref(rent.minimum_balance(TokenAccount::get_packed_len()), TokenAccount::get_packed_len(), &program_id);
        let delegate_key = Pubkey::new_unique();
        let delegate_account = AccountSharedData::new_ref(0, 0, &delegate_key);
        let mismatch_key = Pubkey::new_unique();
        let mismatch_account = AccountSharedData::new_ref(rent.minimum_balance(TokenAccount::get_packed_len()), TokenAccount::get_packed_len(), &program_id);
        let owner_key = Pubkey::new_unique();
        let owner_account = AccountSharedData::new_ref(0, 0, &owner_key);
        let owner2_key = Pubkey::new_unique();
        let owner2_account = AccountSharedData::new_ref(0, 0, &owner2_key);
        let mint_key = Pubkey::new_unique();
        let mint_account = AccountSharedData::new_ref(rent.minimum_balance(Mint::get_packed_len()), Mint::get_packed_len(), &program_id);
        let mint2_key = Pubkey::new_unique();

        // create mint
        process_token_instruction(
            &initialize_mint(&program_id,&mint_key, &owner_key, None, &"Test Token".to_string(), &"TST".to_string(), 2).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
            ],
        ).unwrap();

        // create account
        process_token_instruction(
            &initialize_account(&program_id,&account_key, &mint_key, &owner_key).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // create another account
        process_token_instruction(
            &initialize_account(&program_id,&account2_key, &mint_key, &owner_key).unwrap(),
            &[
                (true, true, account2_key, account2_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // create another account
        process_token_instruction(
            &initialize_account(&program_id,&account3_key, &mint_key, &owner_key).unwrap(),
            &[
                (true, true, account3_key, account3_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // create mismatch account
        process_token_instruction(
            &initialize_account(&program_id,&mismatch_key, &mint_key, &owner_key).unwrap(),
            &[
                (true, true, mismatch_key, mismatch_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        let mut account = TokenAccount::unpack(mismatch_account.try_borrow().unwrap().data()).unwrap();
        account.mint = mint2_key;
        TokenAccount::pack(account, mismatch_account.try_borrow_mut().unwrap().data_as_mut_slice()).unwrap();

        // mint to account
        process_token_instruction(
            &mint_to(&program_id,&mint_key, &account_key, &owner_key, &[], 1000).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
                (true, true, account_key, account_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // missing signer
        assert_eq!(
            Err(InstructionError::MissingRequiredSignature),
            process_token_instruction(
                &transfer(&program_id,&account_key, &account2_key, &owner_key, &[], 1000).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, account2_key, account2_account.clone()),
                    (false, true, owner_key, owner_account.clone()),
                ],
            )
        );

        // mismatch mint
        assert_eq!(
            Err(TokenError::MintMismatch.into()),
            process_token_instruction(
                &transfer(&program_id,&account_key, &mismatch_key, &owner_key, &[], 1000).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, mismatch_key, mismatch_account.clone()),
                    (true, true, owner_key, owner_account.clone()),
                ],
            )
        );

        // missing owner
        assert_eq!(
            Err(TokenError::OwnerMismatch.into()),
            process_token_instruction(
                &transfer(&program_id,&account_key, &account2_key, &owner2_key, &[], 1000).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, account2_key, account2_account.clone()),
                    (true, true, owner2_key, owner2_account.clone()),
                ],
            )
        );

        // transfer
        process_token_instruction(
            &transfer(&program_id,&account_key, &account2_key, &owner_key, &[], 1000).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, account2_key, account2_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // insufficient funds
        assert_eq!(
            Err(TokenError::InsufficientFunds.into()),
            process_token_instruction(
                &transfer(&program_id,&account_key, &account2_key, &owner_key, &[], 1).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, account2_key, account2_account.clone()),
                    (true, true, owner_key, owner_account.clone()),
                ],
            )
        );

        // transfer half back
        process_token_instruction(
            &transfer(&program_id,&account2_key, &account_key, &owner_key, &[], 500).unwrap(),
            &[
                (true, true, account2_key, account2_account.clone()),
                (true, true, account_key, account_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // incorrect decimals
        assert_eq!(
            Err(TokenError::MintDecimalsMismatch.into()),
            process_token_instruction(
                &transfer_checked(
                    &program_id,
                    &account2_key,
                    &mint_key,
                    &account_key,
                    &owner_key,
                    &[],
                    1,
                    10 // <-- incorrect decimals
                ).unwrap(),
                &[
                    (true, true, account2_key, account2_account.clone()),
                    (true, true, mint_key, mint_account.clone()),
                    (true, true, account_key, account_account.clone()),
                    (true, true, owner_key, owner_account.clone()),
                ],
            )
        );

        // incorrect mint
        assert_eq!(
            Err(TokenError::MintMismatch.into()),
            process_token_instruction(
                &transfer_checked(
                    &program_id,
                    &account2_key,
                    &account3_key, // <-- incorrect mint
                    &account_key,
                    &owner_key,
                    &[],
                    1,
                    2
                ).unwrap(),
                &[
                    (true, true, account2_key, account2_account.clone()),
                    (true, true, account3_key, account3_account.clone()),
                    (true, true, account_key, account_account.clone()),
                    (true, true, owner_key, owner_account.clone()),
                ],
            )
        );

        // transfer rest with explicit decimals
        process_token_instruction(
            &transfer_checked(
                &program_id,
                &account2_key,
                &mint_key,
                &account_key,
                &owner_key,
                &[],
                500,
                2,
            ).unwrap(),
            &[
                (true, true, account2_key, account2_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, account_key, account_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // insufficient funds
        assert_eq!(
            Err(TokenError::InsufficientFunds.into()),
            process_token_instruction(
                &transfer(&program_id,&account2_key, &account_key, &owner_key, &[], 1).unwrap(),
                &[
                    (true, true, account2_key, account2_account.clone()),
                    (true, true, account_key, account_account.clone()),
                    (true, true, owner_key, owner_account.clone()),
                ],
            )
        );

        // approve delegate
        process_token_instruction(
            &approve(
                &program_id,
                &account_key,
                &delegate_key,
                &owner_key,
                &[],
                100,
            ).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, delegate_key, delegate_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // transfer via delegate
        process_token_instruction(
            &transfer(&program_id,&account_key, &account2_key, &delegate_key, &[], 100).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, account2_key, account2_account.clone()),
                (true, true, delegate_key, delegate_account.clone()),
            ],
        ).unwrap();

        // insufficient funds approved via delegate
        assert_eq!(
            Err(TokenError::OwnerMismatch.into()),
            process_token_instruction(
                &transfer(&program_id,&account_key, &account2_key, &delegate_key, &[], 100).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, account2_key, account2_account.clone()),
                    (true, true, delegate_key, delegate_account.clone()),
                ],
            )
        );

        // transfer rest
        process_token_instruction(
            &transfer(&program_id,&account_key, &account2_key, &owner_key, &[], 900).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, account2_key, account2_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // approve delegate
        process_token_instruction(
            &approve(
                &program_id,
                &account_key,
                &delegate_key,
                &owner_key,
                &[],
                100,
            ).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, delegate_key, delegate_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // insufficient funds in source account via delegate
        assert_eq!(
            Err(TokenError::InsufficientFunds.into()),
            process_token_instruction(
                &transfer(&program_id,&account_key, &account2_key, &delegate_key, &[], 100).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, account2_key, account2_account.clone()),
                    (true, true, delegate_key, delegate_account.clone()),
                ],
            )
        );
    }

    #[test]
    fn test_self_transfer() {
        let program_id = mundis_sdk::token::program::id();
        let rent = Rent::default();
        let account_key = Pubkey::new_unique();
        let account_account = AccountSharedData::new_ref(rent.minimum_balance(TokenAccount::get_packed_len()), TokenAccount::get_packed_len(), &program_id);
        let account2_key = Pubkey::new_unique();
        let account2_account = AccountSharedData::new_ref(rent.minimum_balance(TokenAccount::get_packed_len()), TokenAccount::get_packed_len(), &program_id);
        let account3_key = Pubkey::new_unique();
        let account3_account = AccountSharedData::new_ref(rent.minimum_balance(TokenAccount::get_packed_len()), TokenAccount::get_packed_len(), &program_id);
        let delegate_key = Pubkey::new_unique();
        let delegate_account = Rc::new(RefCell::new(AccountSharedData::default()));
        let owner_key = Pubkey::new_unique();
        let mut owner_account =  Rc::new(RefCell::new(AccountSharedData::default()));
        let owner2_key = Pubkey::new_unique();
        let mut owner2_account = Rc::new(RefCell::new(AccountSharedData::default()));
        let mint_key = Pubkey::new_unique();
        let mut mint_account = AccountSharedData::new_ref(rent.minimum_balance(Mint::get_packed_len()), Mint::get_packed_len(), &program_id);

        // create mint
        process_token_instruction(
            &initialize_mint(&program_id,&mint_key, &owner_key, None, &"Test Token".to_string(), &"TST".to_string(), 2).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
            ]).unwrap();

        // create account
        process_token_instruction(
            &initialize_account(&program_id,&account_key, &mint_key, &owner_key).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ]).unwrap();

        // create another account
        process_token_instruction(
            &initialize_account(&program_id,&account2_key, &mint_key, &owner_key).unwrap(),
            &[
                (true, true, account2_key, account2_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ]).unwrap();

        // create another account
        process_token_instruction(
            &initialize_account(&program_id,&account3_key, &mint_key, &owner_key).unwrap(),
            &[
                (true, true, account3_key, account3_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ]).unwrap();

        // mint to account
        process_token_instruction(
            &mint_to(&program_id,&mint_key, &account_key, &owner_key, &[], 1000).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
                (true, true, account_key, account_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // transfer
        assert_eq!(
            process_token_instruction(
                &transfer(&program_id,&account_key, &account_key, &owner_key, &[], 1000).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, account_key, account_account.clone()),
                    (true, true, owner_key, owner_account.clone()),
                ],
            ),
            Ok(())
        );

        // no balance change...
        let account = TokenAccount::unpack_unchecked(&account_account.try_borrow().unwrap().data()).unwrap();
        assert_eq!(account.amount, 1000);

        assert_eq!(
            process_token_instruction(
                &transfer_checked(
                    &program_id,
                    &account_key,
                    &mint_key,
                    &account_key,
                    &owner_key,
                    &[],
                    1000,
                    2
                ).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, mint_key, mint_account.clone()),
                    (true, true, account_key, account_account.clone()),
                    (true, true, owner_key, owner_account.clone()),
                ],
            ),
            Ok(())
        );

        // no balance change...
        let account = TokenAccount::unpack_unchecked(&account_account.try_borrow().unwrap().data()).unwrap();
        assert_eq!(account.amount, 1000);

        // missing signer
        assert_eq!(
            process_token_instruction(
                &transfer(&program_id,&account_key, &account_key, &owner_key, &[], 1000).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, account_key, account_account.clone()),
                    (false, true, owner_key, owner_account.clone()),
                ],
            ),
            Err(InstructionError::MissingRequiredSignature),
        );

        // missing signer checked
        assert_eq!(
            process_token_instruction(
                &transfer_checked(
                    &program_id,
                    &account_key,
                    &mint_key,
                    &account_key,
                    &owner_key,
                    &[],
                    1000,
                    2
                ).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, mint_key, mint_account.clone()),
                    (true, true, account_key, account_account.clone()),
                    (false, true, owner_key, owner_account.clone()),
                ],
            ),
            Err(InstructionError::MissingRequiredSignature),
        );

        // missing owner
        assert_eq!(
            process_token_instruction(
                &transfer(&program_id,&account_key, &account_key, &owner2_key, &[], 1000).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, account_key, account_account.clone()),
                    (true, true, owner2_key, owner2_account.clone()),
                ],
            ),
            Err(TokenError::OwnerMismatch.into()),
        );

        // missing owner checked
        assert_eq!(
            process_token_instruction(
                &transfer_checked(
                    &program_id,
                    &account_key,
                    &mint_key,
                    &account_key,
                    &owner2_key,
                    &[],
                    1000,
                    2
                ).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, mint_key, mint_account.clone()),
                    (true, true, account_key, account_account.clone()),
                    (true, true, owner2_key, owner2_account.clone()),
                ],
            ),
            Err(TokenError::OwnerMismatch.into()),
        );

        // insufficient funds
        assert_eq!(
            process_token_instruction(
                &transfer(&program_id,&account_key, &account_key, &owner_key, &[], 1001).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, account_key, account_account.clone()),
                    (true, true, owner_key, owner_account.clone()),
                ],
            ),
            Err(TokenError::InsufficientFunds.into()),
        );

        // insufficient funds checked
        assert_eq!(
            process_token_instruction(
                &transfer_checked(
                    &program_id,
                    &account_key,
                    &mint_key,
                    &account_key,
                    &owner_key,
                    &[],
                    1001,
                    2
                ).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, mint_key, mint_account.clone()),
                    (true, true, account_key, account_account.clone()),
                    (true, true, owner_key, owner_account.clone()),
                ],
            ),
            Err(TokenError::InsufficientFunds.into()),
        );

        // incorrect decimals
        assert_eq!(
            process_token_instruction(
                &transfer_checked(
                    &program_id,
                    &account_key,
                    &mint_key,
                    &account_key,
                    &owner_key,
                    &[],
                    1,
                    10 // <-- incorrect decimals
                ).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, mint_key, mint_account.clone()),
                    (true, true, account_key, account_account.clone()),
                    (true, true, owner_key, owner_account.clone()),
                ],
            ),
            Err(TokenError::MintDecimalsMismatch.into()),
        );

        // incorrect mint
        assert_eq!(
            process_token_instruction(
                &transfer_checked(
                    &program_id,
                    &account_key,
                    &account3_key, // <-- incorrect mint
                    &account_key,
                    &owner_key,
                    &[],
                    1,
                    2
                ).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, account3_key, account3_account.clone()), // <-- incorrect mint
                    (true, true, account_key, account_account.clone()),
                    (true, true, owner_key, owner_account.clone()),
                ],
            ),
            Err(TokenError::MintMismatch.into()),
        );

        // approve delegate
        process_token_instruction(
            &approve(
                &program_id,
                &account_key,
                &delegate_key,
                &owner_key,
                &[],
                100,
            ).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, delegate_key, delegate_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // transfer via delegate
        process_token_instruction(
            &transfer(&program_id,&account_key, &account_key, &delegate_key, &[], 100).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, account_key, account_account.clone()),
                (true, true, delegate_key, delegate_account.clone()),
            ],
        ).unwrap();

        // no balance change...
        let account = TokenAccount::unpack_unchecked(&account_account.try_borrow().unwrap().data()).unwrap();
        assert_eq!(account.amount, 1000);
        assert_eq!(account.delegated_amount, 100);

        // delegate transfer checked
        assert_eq!(
            process_token_instruction(
                &transfer_checked(
                    &program_id,
                    &account_key,
                    &mint_key,
                    &account_key,
                    &delegate_key,
                    &[],
                    100,
                    2
                ).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, mint_key, mint_account.clone()),
                    (true, true, account_key, account_account.clone()),
                    (true, true, delegate_key, delegate_account.clone()),
                ],
            ),
            Ok(())
        );

        // no balance change...
        let account = TokenAccount::unpack_unchecked(&account_account.try_borrow().unwrap().data()).unwrap();
        assert_eq!(account.amount, 1000);
        assert_eq!(account.delegated_amount, 100);

        // delegate insufficient funds
        assert_eq!(
            process_token_instruction(
                &transfer(&program_id,&account_key, &account_key, &delegate_key, &[], 101).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, account_key, account_account.clone()),
                    (true, true, delegate_key, delegate_account.clone()),
                ],
            ),
            Err(TokenError::InsufficientFunds.into()),
        );

        // delegate insufficient funds checked
        assert_eq!(
            process_token_instruction(
                &transfer_checked(
                    &program_id,
                    &account_key,
                    &mint_key,
                    &account_key,
                    &delegate_key,
                    &[],
                    101,
                    2
                ).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, mint_key, mint_account.clone()),
                    (true, true, account_key, account_account.clone()),
                    (true, true, delegate_key, delegate_account.clone()),
                ],
            ),
            Err(TokenError::InsufficientFunds.into()),
        );

        // owner transfer with delegate assigned
        assert_eq!(
            process_token_instruction(
                &transfer(&program_id,&account_key, &account_key, &owner_key, &[], 1000).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, account_key, account_account.clone()),
                    (true, true, owner_key, owner_account.clone()),
                ],
            ),
            Ok(())
        );

        // no balance change...
        let account = TokenAccount::unpack_unchecked(&account_account.try_borrow().unwrap().data()).unwrap();
        assert_eq!(account.amount, 1000);

        // owner transfer with delegate assigned checked
        assert_eq!(
            process_token_instruction(
                &transfer_checked(
                    &program_id,
                    &account_key,
                    &mint_key,
                    &account_key,
                    &owner_key,
                    &[],
                    1000,
                    2
                ).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, mint_key, mint_account.clone()),
                    (true, true, account_key, account_account.clone()),
                    (true, true, owner_key, owner_account.clone()),
                ],
            ),
            Ok(())
        );

        // no balance change...
        let account = TokenAccount::unpack_unchecked(&account_account.try_borrow().unwrap().data()).unwrap();
        assert_eq!(account.amount, 1000);
    }

    #[test]
    fn test_mintable_token_with_zero_supply() {
        let program_id = mundis_sdk::token::program::id();
        let rent = Rent::default();
        let account_key = Pubkey::new_unique();
        let account_account = AccountSharedData::new_ref(rent.minimum_balance(TokenAccount::get_packed_len()), TokenAccount::get_packed_len(), &program_id);
        let owner_key = Pubkey::new_unique();
        let owner_account= Rc::new(RefCell::new(AccountSharedData::default()));
        let mint_key = Pubkey::new_unique();
        let mut mint_account = AccountSharedData::new_ref(rent.minimum_balance(Mint::get_packed_len()), Mint::get_packed_len(), &program_id);

        // create mint-able token with zero supply
        let decimals = 2;
        process_token_instruction(
            &initialize_mint(&program_id,&mint_key, &owner_key, None, &"".to_string(), &"".to_string(), decimals).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
            ]).unwrap();

        let mint = Mint::unpack_unchecked(&mint_account.try_borrow().unwrap().data()).unwrap();
        assert_eq!(
            mint,
            Mint {
                mint_authority: Some(owner_key),
                name: puffed_out_string(&"".to_string(), MAX_NAME_LENGTH),
                symbol: puffed_out_string(&"".to_string(), MAX_SYMBOL_LENGTH),
                supply: 0,
                decimals,
                is_initialized: true,
                freeze_authority: None,
            }
        );

        // create account
        process_token_instruction(
            &initialize_account(&program_id,&account_key, &mint_key, &owner_key).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ]).unwrap();

        // mint to
        process_token_instruction(
            &mint_to(&program_id,&mint_key, &account_key, &owner_key, &[], 42).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
                (true, true, account_key, account_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        let _ = Mint::unpack(&mint_account.try_borrow().unwrap().data()).unwrap();
        let account = TokenAccount::unpack_unchecked(&account_account.try_borrow().unwrap().data()).unwrap();
        assert_eq!(account.amount, 42);

        // mint to 2
        process_token_instruction(
            &mint_to(&program_id,&mint_key, &account_key, &owner_key, &[], 42).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
                (true, true, account_key, account_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        let _ = Mint::unpack(&mint_account.try_borrow().unwrap().data()).unwrap();
        let account = TokenAccount::unpack_unchecked(&account_account.try_borrow().unwrap().data()).unwrap();
        assert_eq!(account.amount, 84);
    }

   #[test]
    fn test_initialize_account2() {
        let program_id = mundis_sdk::token::program::id();
        let rent = Rent::default();
        let account_key = Pubkey::new_unique();
        let account_account = AccountSharedData::new_ref(rent.minimum_balance(TokenAccount::get_packed_len()), TokenAccount::get_packed_len(), &program_id);
        let account2_key = Pubkey::new_unique();
        let account2_account = AccountSharedData::new_ref(rent.minimum_balance(TokenAccount::get_packed_len()), TokenAccount::get_packed_len(), &program_id);
        let owner_key = Pubkey::new_unique();
        let owner_account = AccountSharedData::new_ref(0, 0, &owner_key);
        let mint_key = Pubkey::new_unique();
        let mint_account = AccountSharedData::new_ref(rent.minimum_balance(Mint::get_packed_len()), Mint::get_packed_len(), &program_id);

        // create mint
        process_token_instruction(
            &initialize_mint(&program_id,&mint_key, &owner_key, None, &"Test Token".to_string(), &"TST".to_string(), 2).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
            ]).unwrap();

        process_token_instruction(
            &initialize_account(&program_id,&account_key, &mint_key, &owner_key).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ]).unwrap();

        process_token_instruction(
            &initialize_account2(&program_id,&account_key, &mint_key, &owner_key).unwrap(),
            &[
                (true, true, account2_key, account2_account.clone()),
                (true, true, mint_key, mint_account.clone()),
            ]).unwrap();

        assert_eq!(account_account.take().data(), account2_account.take().data());
    }

    #[test]
    fn test_multisig() {
        let program_id = mundis_sdk::token::program::id();
        let rent = Rent::default();
        let mint_key = Pubkey::new_unique();
        let mint_account = AccountSharedData::new_ref(rent.minimum_balance(Mint::get_packed_len()), Mint::get_packed_len(), &program_id);

        let multisig_key = Pubkey::new_unique();
        let multisig_account = AccountSharedData::new_ref(rent.minimum_balance(Multisig::get_packed_len()), Multisig::get_packed_len(), &program_id);

        let multisig_delegate_key = Pubkey::new_unique();
        let multisig_delegate_account = AccountSharedData::new_ref(rent.minimum_balance(Multisig::get_packed_len()), Multisig::get_packed_len(), &program_id);

        let signer_keys = vec![Pubkey::new_unique(); MAX_SIGNERS];
        let signer_key_refs: Vec<&Pubkey> = signer_keys.iter().collect();
        let mut signer_accounts = vec![];
        for _ in 0..MAX_SIGNERS {
            signer_accounts.push(AccountSharedData::new_ref(0, 0, &program_id));
        }

        // single signer
        process_token_instruction(
            &initialize_multisig(&program_id,&multisig_key, &[&signer_keys[0]], 1).unwrap(),
            &[
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone())
            ],
        ).unwrap();

        // multiple signers
        process_token_instruction(
            &initialize_multisig(&program_id,&multisig_key, &signer_key_refs, MAX_SIGNERS as u8).unwrap(),
            &[
                (true, true, multisig_delegate_key, multisig_delegate_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone()),
                (true, true, signer_keys[1], signer_accounts[1].clone()),
                (true, true, signer_keys[2], signer_accounts[2].clone()),
                (true, true, signer_keys[3], signer_accounts[3].clone()),
                (true, true, signer_keys[4], signer_accounts[4].clone()),
                (true, true, signer_keys[5], signer_accounts[5].clone()),
                (true, true, signer_keys[6], signer_accounts[6].clone()),
                (true, true, signer_keys[7], signer_accounts[7].clone()),
                (true, true, signer_keys[8], signer_accounts[8].clone()),
                (true, true, signer_keys[9], signer_accounts[9].clone()),
                (true, true, signer_keys[10], signer_accounts[10].clone()),
            ],
        ).unwrap();

        // create new mint with multisig owner
        process_token_instruction(
            &initialize_mint(&program_id,&mint_key, &multisig_key, None, &"Test Token".to_string(), &"TST".to_string(), 2).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
            ],
        ).unwrap();

        // create account with multisig owner
        let account_key = Pubkey::new_unique();
        let account_account = AccountSharedData::new_ref(rent.minimum_balance(TokenAccount::get_packed_len()), TokenAccount::get_packed_len(), &account_key);
        process_token_instruction(
            &initialize_account(&program_id,&account_key, &mint_key, &multisig_key).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
            ],
        ).unwrap();

        // create another account with multisig owner
        let account2_key = Pubkey::new_unique();
        let account2_account = AccountSharedData::new_ref(rent.minimum_balance(TokenAccount::get_packed_len()), TokenAccount::get_packed_len(), &account2_key);
        process_token_instruction(
            &initialize_account(&program_id,&account2_key, &mint_key, &multisig_delegate_key).unwrap(),
            &[
                (true, true, account2_key, account2_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
            ],
        ).unwrap();

        // mint to account
        process_token_instruction(
            &mint_to(&program_id,&mint_key, &account_key, &multisig_key, &[&signer_keys[0]], 1000).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
                (true, true, account_key, account_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone())
            ],
        ).unwrap();

        // approve
        process_token_instruction(
            &approve(&program_id,&account_key, &multisig_delegate_key, &multisig_key, &[&signer_keys[0]], 100).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, multisig_delegate_key, multisig_delegate_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone()),
            ],
        ).unwrap();

        // transfer
        process_token_instruction(
            &transfer(&program_id,&account_key, &account2_key, &multisig_key, &[&signer_keys[0]], 42).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, account2_key, account2_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone()),
            ],
        ).unwrap();

        // transfer via delegate
        process_token_instruction(
            &transfer(&program_id,&account_key, &account2_key, &multisig_delegate_key, &signer_key_refs, 42).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, account2_key, account2_account.clone()),
                (true, true, multisig_delegate_key, multisig_delegate_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone()),
                (true, true, signer_keys[1], signer_accounts[1].clone()),
                (true, true, signer_keys[2], signer_accounts[2].clone()),
                (true, true, signer_keys[3], signer_accounts[3].clone()),
                (true, true, signer_keys[4], signer_accounts[4].clone()),
                (true, true, signer_keys[5], signer_accounts[5].clone()),
                (true, true, signer_keys[6], signer_accounts[6].clone()),
                (true, true, signer_keys[7], signer_accounts[7].clone()),
                (true, true, signer_keys[8], signer_accounts[8].clone()),
                (true, true, signer_keys[9], signer_accounts[9].clone()),
                (true, true, signer_keys[10], signer_accounts[10].clone()),
            ],
        ).unwrap();

        // mint to
        process_token_instruction(
            &mint_to(&program_id,&mint_key, &account2_key, &multisig_key, &[&signer_keys[0]], 42).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
                (true, true, account2_key, account2_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone())
            ],
        ).unwrap();

        // burn
        process_token_instruction(
            &burn(&program_id,&account_key, &mint_key, &multisig_key, &[&signer_keys[0]], 42).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone())
            ],
        ).unwrap();

        // burn via delegate
        process_token_instruction(
            &burn(&program_id,&account_key, &mint_key, &multisig_delegate_key, &signer_key_refs, 42).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, multisig_delegate_key, multisig_delegate_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone()),
                (true, true, signer_keys[1], signer_accounts[1].clone()),
                (true, true, signer_keys[2], signer_accounts[2].clone()),
                (true, true, signer_keys[3], signer_accounts[3].clone()),
                (true, true, signer_keys[4], signer_accounts[4].clone()),
                (true, true, signer_keys[5], signer_accounts[5].clone()),
                (true, true, signer_keys[6], signer_accounts[6].clone()),
                (true, true, signer_keys[7], signer_accounts[7].clone()),
                (true, true, signer_keys[8], signer_accounts[8].clone()),
                (true, true, signer_keys[9], signer_accounts[9].clone()),
                (true, true, signer_keys[10], signer_accounts[10].clone()),
            ],
        ).unwrap();

        // freeze account
        let account3_key = Pubkey::new_unique();
        let account3_account = AccountSharedData::new_ref(rent.minimum_balance(TokenAccount::get_packed_len()), TokenAccount::get_packed_len(), &program_id);
        let mint2_key = Pubkey::new_unique();
        let mint2_account = AccountSharedData::new_ref(rent.minimum_balance(Mint::get_packed_len()), Mint::get_packed_len(), &program_id);
        process_token_instruction(
            &initialize_mint(&program_id,&mint2_key, &multisig_key, Some(&multisig_key), &"Test Token".to_string(), &"TST".to_string(), 2).unwrap(),
            &[
                (true, true, mint2_key, mint2_account.clone()),
            ],
        ).unwrap();

        let owner_key = Pubkey::new_unique();
        let owner_account = AccountSharedData::new_ref(0, 0, &owner_key);
        process_token_instruction(
            &initialize_account(&program_id,&account3_key, &mint2_key, &owner_key).unwrap(),
            &[
                (true, true, account3_key, account3_account.clone()),
                (true, true, mint2_key, mint2_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();
        process_token_instruction(
            &mint_to(&program_id,&mint2_key, &account3_key, &multisig_key, &[&signer_keys[0]], 1000).unwrap(),
            &[
                (true, true, mint2_key, mint2_account.clone()),
                (true, true, account3_key, account3_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone())
            ],
        ).unwrap();
        process_token_instruction(
            &freeze_account(&program_id,&account3_key, &mint2_key, &multisig_key, &[&signer_keys[0]]).unwrap(),
            &[
                (true, true, account3_key, account3_account.clone()),
                (true, true, mint2_key, mint2_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone())
            ],
        ).unwrap();

        // do SetAuthority on mint
        process_token_instruction(
            &set_authority(&program_id,&mint_key, Some(&owner_key), AuthorityType::MintTokens, &multisig_key, &[&signer_keys[0]]).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone())
            ],
        ).unwrap();

        // do SetAuthority on account
        process_token_instruction(
            &set_authority(&program_id,&account_key, Some(&owner_key), AuthorityType::AccountOwner, &multisig_key, &[&signer_keys[0]]).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone())
            ],
        ).unwrap();
    }

    // TODO: Mundis: add remaining tests
}