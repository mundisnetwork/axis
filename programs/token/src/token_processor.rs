use num_traits::FromPrimitive;

use mundis_program::{
    decode_error::DecodeError,
    instruction::InstructionError,
    program_pack::IsInitialized,
    pubkey::Pubkey,
};
use mundis_program_runtime::invoke_context::InvokeContext;
use mundis_sdk::account::{ReadableAccount, WritableAccount};
use mundis_sdk::keyed_account::{keyed_account_at_index, KeyedAccount, next_keyed_account};
use mundis_sdk::program_utils::limited_deserialize;

use crate::{
    error::TokenError,
    state::{Account, AccountState, Mint, Multisig},
    token_instruction::{AuthorityType, is_valid_signer_index, MAX_SIGNERS, TokenInstruction},
};
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
                decimals,
                mint_authority,
                freeze_authority,
            } => {
                println!("Instruction: InitializeMint");
                Self::process_initialize_mint(accounts, decimals, mint_authority, freeze_authority)
            }
            TokenInstruction::InitializeAccount => {
                println!("Instruction: InitializeAccount");
                Self::process_initialize_account(accounts)
            }
            TokenInstruction::InitializeAccount2 { owner } => {
                println!("Instruction: InitializeAccount2");
                Self::process_initialize_account2(accounts, owner)
            }
            TokenInstruction::InitializeMultisig { m } => {
                println!("Instruction: InitializeMultisig");
                Self::process_initialize_multisig(accounts, m)
            }
            TokenInstruction::Transfer { amount } => {
                println!("Instruction: Transfer");
                Self::process_transfer(program_id, accounts, amount, None)
            }
            TokenInstruction::Approve { amount } => {
                println!("Instruction: Approve");
                Self::process_approve(program_id, accounts, amount, None)
            }
            TokenInstruction::Revoke => {
                println!("Instruction: Revoke");
                Self::process_revoke(program_id, accounts)
            }
            TokenInstruction::SetAuthority {
                authority_type,
                new_authority,
            } => {
                println!("Instruction: SetAuthority");
                Self::process_set_authority(program_id, accounts, authority_type, new_authority)
            }
            TokenInstruction::MintTo { amount } => {
                println!("Instruction: MintTo");
                Self::process_mint_to(program_id, accounts, amount, None)
            }
            TokenInstruction::Burn { amount } => {
                println!("Instruction: Burn");
                Self::process_burn(program_id, accounts, amount, None)
            }
            TokenInstruction::CloseAccount => {
                println!("Instruction: CloseAccount");
                Self::process_close_account(program_id, accounts)
            }
            TokenInstruction::FreezeAccount => {
                println!("Instruction: FreezeAccount");
                Self::process_toggle_freeze_account(program_id, accounts, true)
            }
            TokenInstruction::ThawAccount => {
                println!("Instruction: ThawAccount");
                Self::process_toggle_freeze_account(program_id, accounts, false)
            }
            TokenInstruction::TransferChecked { amount, decimals } => {
                println!("Instruction: TransferChecked");
                Self::process_transfer(program_id, accounts, amount, Some(decimals))
            }
            TokenInstruction::ApproveChecked { amount, decimals } => {
                println!("Instruction: ApproveChecked");
                Self::process_approve(program_id, accounts, amount, Some(decimals))
            }
            TokenInstruction::MintToChecked { amount, decimals } => {
                println!("Instruction: MintToChecked");
                Self::process_mint_to(program_id, accounts, amount, Some(decimals))
            }
            TokenInstruction::BurnChecked { amount, decimals } => {
                println!("Instruction: BurnChecked");
                Self::process_burn(program_id, accounts, amount, Some(decimals))
            }
            TokenInstruction::SyncNative => {
                println!("Instruction: SyncNative");
                Self::process_sync_native(program_id, accounts)
            }
        }
    }

    pub fn process_initialize_mint(
        accounts: &[KeyedAccount],
        decimals: u8,
        mint_authority: Pubkey,
        freeze_authority: Option<Pubkey>,
    ) -> Result<(), InstructionError> {
        let accounts_iter = &mut accounts.iter();
        let mint_info = next_keyed_account(accounts_iter)?;

        let mut mint = Mint::unpack(mint_info.try_account_ref()?.data())
            .unwrap_or(Mint::default());
        if mint.is_initialized {
            return Err(TokenError::AlreadyInUse.into());
        }

        mint.mint_authority = Some(mint_authority);
        mint.decimals = decimals;
        mint.is_initialized = true;
        mint.freeze_authority = freeze_authority;

        mint.pack(mint_info.try_account_ref_mut()?.data_mut())
    }

    fn _process_initialize_account(
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
        let mut account = Account::unpack(new_account_info.try_account_ref()?.data())
            .unwrap_or(Account::default());
        if account.is_initialized() {
            return Err(TokenError::AlreadyInUse.into());
        }

        if *mint_info.unsigned_key() != crate::native_mint::id() {
            let mint = Mint::unpack(mint_info.try_account_ref()?.data())
                .unwrap_or(Mint::default());

            if !mint.is_initialized() {
                return Err(TokenError::InvalidMint.into());
            }
        }

        account.mint = *mint_info.unsigned_key();
        account.owner = *owner;
        account.delegate = Option::None;
        account.delegated_amount = 0;
        account.state = AccountState::Initialized;

        if *mint_info.unsigned_key() == crate::native_mint::id() {
            account.is_native = true;
            account.amount = new_account_info.lamports()?;
        } else {
            account.is_native = false;
            account.amount = 0;
        };

        account.pack(new_account_info.try_account_ref_mut()?.data_mut())
    }

    /// Processes an [InitializeAccount](enum.TokenInstruction.html) instruction.
    pub fn process_initialize_account(accounts: &[KeyedAccount]) -> Result<(), InstructionError> {
        Self::_process_initialize_account(accounts, None)
    }

    /// Processes an [InitializeAccount2](enum.TokenInstruction.html) instruction.
    pub fn process_initialize_account2(accounts: &[KeyedAccount], owner: Pubkey) -> Result<(), InstructionError> {
        Self::_process_initialize_account(accounts, Some(&owner))
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

        let signer_infos = &accounts[1..];
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

        multisig.pack(multisig_info.try_account_ref_mut()?.data_mut())
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

        let mut source_account = Account::unpack(source_account_info.try_account_ref()?.data())?;
        let mut dest_account = Account::unpack(dest_account_info.try_account_ref()?.data())?;

        if source_account.is_frozen() || dest_account.is_frozen() {
            return Err(TokenError::AccountFrozen.into());
        }
        if source_account.amount < amount {
            return Err(TokenError::InsufficientFunds.into());
        }
        if source_account.mint != dest_account.mint {
            return Err(TokenError::MintMismatch.into());
        }

        if let Some((mint_info, expected_decimals)) = expected_mint_info {
            if source_account.mint != *mint_info.unsigned_key() {
                return Err(TokenError::MintMismatch.into());
            }

            let mint = Mint::unpack(&mint_info.try_account_ref()?.data())?;
            if expected_decimals != mint.decimals {
                return Err(TokenError::MintDecimalsMismatch.into());
            }
        }

        let self_transfer = source_account_info.unsigned_key() == dest_account_info.unsigned_key();

        match source_account.delegate {
            Some(ref delegate) if authority_info.unsigned_key() == delegate => {
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

        source_account.pack(source_account_info.try_account_ref_mut()?.data_mut())
            .and_then(|_| dest_account.pack(dest_account_info.try_account_ref_mut()?.data_mut()))
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

        let mut source_account = Account::unpack(source_account_info.try_account_ref()?.data())?;

        if source_account.is_frozen() {
            return Err(TokenError::AccountFrozen.into());
        }

        if let Some((mint_info, expected_decimals)) = expected_mint_info {
            if source_account.mint != *mint_info.unsigned_key() {
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

        source_account.pack(source_account_info.try_account_ref_mut()?.data_mut())
    }

    /// Processes an [Revoke](enum.TokenInstruction.html) instruction.
    pub fn process_revoke(
        program_id: &Pubkey,
        accounts: &[KeyedAccount],
    ) -> Result<(), InstructionError> {
        let account_info_iter = &mut accounts.iter();
        let source_account_info = next_keyed_account(account_info_iter)?;

        let mut source_account = Account::unpack(source_account_info.try_account_ref()?.data())?;

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

        source_account.pack(source_account_info.try_account_ref_mut()?.data_mut())
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

        if account_info.data_len()? == Account::packed_len() {
            let mut account = Account::unpack(account_info.try_account_ref()?.data())?;

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
            account.pack(account_info.try_account_ref_mut()?.data_mut())
        } else if account_info.data_len()? == Mint::packed_len() {
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
            mint.pack(account_info.try_account_ref_mut()?.data_mut())
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

        let mut dest_account = Account::unpack(dest_account_info.try_account_ref()?.data())?;

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

        dest_account.amount = dest_account
            .amount
            .checked_add(amount)
            .ok_or(TokenError::Overflow)?;

        mint.supply = mint
            .supply
            .checked_add(amount)
            .ok_or(TokenError::Overflow)?;

        dest_account.pack(dest_account_info.try_account_ref_mut()?.data_mut())
            .and_then(|_| mint.pack(mint_info.try_account_ref_mut()?.data_mut()))
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

        let mut source_account = Account::unpack(source_account_info.try_account_ref()?.data())?;

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
        if mint_info.unsigned_key() != &source_account.mint {
            return Err(TokenError::MintMismatch.into());
        }

        if let Some(expected_decimals) = expected_decimals {
            if expected_decimals != mint.decimals {
                return Err(TokenError::MintDecimalsMismatch.into());
            }
        }

        match source_account.delegate {
            Some(ref delegate) if authority_info.unsigned_key() == delegate => {
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

        source_account.amount = source_account
            .amount
            .checked_sub(amount)
            .ok_or(TokenError::Overflow)?;
        mint.supply = mint
            .supply
            .checked_sub(amount)
            .ok_or(TokenError::Overflow)?;

        source_account.pack(source_account_info.try_account_ref_mut()?.data_mut())
            .and_then(|_| mint.pack(mint_info.try_account_ref_mut()?.data_mut()))
    }

    /// Processes a [CloseAccount](enum.TokenInstruction.html) instruction.
    pub fn process_close_account(
        program_id: &Pubkey,
        accounts: &[KeyedAccount],
    ) -> Result<(), InstructionError> {
        let account_info_iter = &mut accounts.iter();
        let source_account_info = next_keyed_account(account_info_iter)?;
        let dest_account_info = next_keyed_account(account_info_iter)?;
        let authority_info = next_keyed_account(account_info_iter)?;

        let mut source_account = Account::unpack(source_account_info.try_account_ref()?.data())?;
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
        dest_account_info.try_account_ref_mut()?.set_lamports(dest_starting_lamports
            .checked_add(source_account_info.lamports()?)
            .ok_or(TokenError::Overflow)?
        );

        source_account_info.try_account_ref_mut()?.set_lamports(0);
        source_account.amount = 0;

        source_account.pack(source_account_info.try_account_ref_mut()?.data_mut())
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

        let mut source_account = Account::unpack(source_account_info.try_account_ref()?.data())?;

        if freeze && source_account.is_frozen() || !freeze && !source_account.is_frozen() {
            return Err(TokenError::InvalidState.into());
        }
        if source_account.is_native() {
            return Err(TokenError::NativeNotSupported.into());
        }
        if mint_info.unsigned_key() != &source_account.mint {
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

        source_account.pack(source_account_info.try_account_ref_mut()?.data_mut())
    }

    /// Processes a [SyncNative](enum.TokenInstruction.html) instruction
    pub fn process_sync_native(
        program_id: &Pubkey,
        accounts: &[KeyedAccount],
    ) -> Result<(), InstructionError> {
        let account_info_iter = &mut accounts.iter();

        let native_account_info = next_keyed_account(account_info_iter)?;

        if native_account_info.owner()? != *program_id {
            return Err(InstructionError::IncorrectProgramId);
        }

        let native_account = Account::unpack(native_account_info.try_account_ref()?.data())?;

        if !native_account.is_native {
            return Err(TokenError::NonNativeNotSupported.into());
        }

        native_account.pack(native_account_info.try_account_ref_mut()?.data_mut())
    }

    /// Validates owner(s) are present
    pub fn validate_owner(
        program_id: &Pubkey,
        expected_owner: &Pubkey,
        owner_account_info: &KeyedAccount,
        signers: &[KeyedAccount],
    ) -> Result<(), InstructionError> {
        if expected_owner != owner_account_info.unsigned_key() {
            return Err(TokenError::OwnerMismatch.into());
        }
        if *program_id == owner_account_info.owner()?
            && owner_account_info.data_len()? == Multisig::packed_len()
        {
            let multisig = Multisig::unpack(owner_account_info.try_account_ref()?.data())?;
            let mut num_signers = 0;
            let mut matched = [false; MAX_SIGNERS];
            for signer in signers.iter() {
                for (position, key) in multisig.signers[0..multisig.n as usize].iter().enumerate() {
                    if key == signer.unsigned_key() && !matched[position] {
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

    use mundis_program::instruction::{Instruction, InstructionError};
    use mundis_program::pubkey::Pubkey;
    use mundis_program_runtime::invoke_context::mock_process_instruction;
    use mundis_sdk::account::{AccountSharedData, ReadableAccount, WritableAccount};

    use crate::error::{PrintInstructionError, TokenError};
    use crate::state::{Account, Mint, Multisig};
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
        assert_ne!(Mint::packed_len(), 0);
        assert_ne!(Mint::packed_len(), Account::packed_len());
        assert_ne!(Mint::packed_len(), Multisig::packed_len());
        assert_ne!(Account::packed_len(), 0);
        assert_ne!(Account::packed_len(), Multisig::packed_len());
        assert_ne!(Multisig::packed_len(), 0);
    }

    #[test]
    fn test_initialize_mint() {
        let owner_key = Pubkey::new_unique();
        let mint_key = Pubkey::new_unique();
        let mint_account = AccountSharedData::new_ref(0, Mint::packed_len(), &mint_key);

        // create new mint
        process_token_instruction(
            &initialize_mint(&mint_key, &owner_key, None, 2).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
            ]).unwrap();

        // create twice
        assert_eq!(
            Err(TokenError::AlreadyInUse.into()),
            process_token_instruction(
                &initialize_mint(&mint_key, &owner_key, None, 2).unwrap(),
                &[
                    (true, true, mint_key, mint_account.clone()),
                ])
        );
    }

    #[test]
    fn test_initialize_mint_account() {
        let account_key = Pubkey::new_unique();
        let account_account = AccountSharedData::new_ref(0, Account::packed_len(), &account_key);
        let owner_key = Pubkey::new_unique();
        let owner_account = AccountSharedData::new_ref(0, 0, &owner_key);
        let mint_key = Pubkey::new_unique();
        let mint_account = AccountSharedData::new_ref(0, Mint::packed_len(), &mint_key);

        // mint is not valid (not initialized)
        assert_eq!(
            Err(TokenError::InvalidMint.into()),
            process_token_instruction(
                &initialize_account(&account_key, &mint_key, &owner_key).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, mint_key, mint_account.clone()),
                    (true, true, owner_key, owner_account.clone()),
                ])
        );

        // create mint
        process_token_instruction(
            &initialize_mint(&mint_key, &owner_key, None, 2).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
            ]).unwrap();

        // create account
        process_token_instruction(
            &initialize_account(&account_key, &mint_key, &owner_key).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ]).unwrap();

        // create twice
        assert_eq!(
            Err(TokenError::AlreadyInUse.into()),
            process_token_instruction(
                &initialize_account(&account_key, &mint_key, &owner_key).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, mint_key, mint_account.clone()),
                    (true, true, owner_key, owner_account.clone()),
                ])
        );
    }

    #[test]
    fn test_transfer_dups() {
        let account1_key = Pubkey::new_unique();
        let account1_account = AccountSharedData::new_ref(100, Account::packed_len(), &account1_key);
        let account2_key = Pubkey::new_unique();
        let account2_account = AccountSharedData::new_ref(100, Account::packed_len(), &account2_key);
        let account3_key = Pubkey::new_unique();
        let account3_account = AccountSharedData::new_ref(100, Account::packed_len(), &account3_key);
        let account4_key = Pubkey::new_unique();
        let account4_account = AccountSharedData::new_ref(100, Account::packed_len(), &account4_key);
        let owner_key = Pubkey::new_unique();
        let owner_account = AccountSharedData::new_ref(100, Account::packed_len(), &owner_key);
        let mint_key = Pubkey::new_unique();
        let mint_account = AccountSharedData::new_ref(0, Mint::packed_len(), &mint_key);
        let multisig_key = Pubkey::new_unique();
        let multisig_account = AccountSharedData::new_ref(0, Multisig::packed_len(), &multisig_key);

        // create mint
        process_token_instruction(
            &initialize_mint(&mint_key, &owner_key, None, 2).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
            ],
        ).unwrap();

        // create account
        process_token_instruction(
            &initialize_account(&account1_key, &mint_key, &account1_key).unwrap(),
            &[
                (true, true, account1_key, account1_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, account1_key, account1_account.clone()),
            ],
        ).unwrap();

        // create another account
        process_token_instruction(
            &initialize_account(&account2_key, &mint_key, &owner_key).unwrap(),
            &[
                (true, true, account2_key, account2_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // mint to account
        process_token_instruction(
            &mint_to(&mint_key, &account1_key, &owner_key, &[], 1000).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
                (true, true, account1_key, account1_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // source-owner transfer
        process_token_instruction(
            &transfer(&account1_key, &account2_key, &account1_key, &[], 500,).unwrap(),
            &[
                (true, true, account1_key, account1_account.clone()),
                (true, true, account2_key, account2_account.clone()),
                (true, true, account1_key, account1_account.clone()),
            ],
        ).unwrap();

        // source-delegate transfer
        let mut account = Account::unpack(account1_account.try_borrow().unwrap().data()).unwrap();
        account.amount = 1000;
        account.delegated_amount = 1000;
        account.delegate = Some(account1_key);
        account.owner = owner_key;
        account.pack(account1_account.try_borrow_mut().unwrap().data_mut()).unwrap();
        process_token_instruction(
            &transfer(&account1_key, &account2_key, &account1_key, &[], 500,).unwrap(),
            &[
                (true, true, account1_key, account1_account.clone()),
                (true, true, account2_key, account2_account.clone()),
                (true, true, account1_key, account1_account.clone()),
            ],
        ).unwrap();

        // source-delegate TransferChecked
        process_token_instruction(
            &transfer_checked(
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
            &initialize_account(&account3_key, &mint_key, &account2_key).unwrap(),
            &[
                (true, true, account3_key, account3_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, account2_key, account2_account.clone()),
            ],
        ).unwrap();
        process_token_instruction(
            &mint_to(&mint_key, &account3_key, &owner_key, &[], 1000).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
                (true, true, account3_key, account3_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();
        process_token_instruction(
            &transfer(&account3_key, &account2_key, &account2_key, &[], 500,).unwrap(),
            &[
                (true, true, account3_key, account3_account.clone()),
                (true, true, account2_key, account2_account.clone()),
                (true, true, account2_key, account2_account.clone()),
            ],
        ).unwrap();

        // destination-owner TransferChecked
        process_token_instruction(
            &transfer_checked(
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
            &initialize_multisig(&multisig_key, &[&account4_key], 1).unwrap(),
            &[
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, account4_key, account4_account.clone())
            ],
        ).unwrap();
        process_token_instruction(
            &initialize_account(&account4_key, &mint_key, &multisig_key).unwrap(),
            &[
                (true, true, account4_key, account4_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
            ],
        ).unwrap();
        process_token_instruction(
            &mint_to(&mint_key, &account4_key, &owner_key, &[], 1000).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
                (true, true, account4_key, account4_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // source-multisig-signer transfer
        process_token_instruction(
            &transfer(&account4_key, &account2_key, &multisig_key, &[&account4_key], 500,).unwrap(),
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
        let account_key = Pubkey::new_unique();
        let account_account = AccountSharedData::new_ref(100, Account::packed_len(), &account_key);
        let account2_key = Pubkey::new_unique();
        let account2_account = AccountSharedData::new_ref(100, Account::packed_len(), &account2_key);
        let account3_key = Pubkey::new_unique();
        let account3_account = AccountSharedData::new_ref(100, Account::packed_len(), &account3_key);
        let delegate_key = Pubkey::new_unique();
        let delegate_account = AccountSharedData::new_ref(0, 0, &delegate_key);
        let mismatch_key = Pubkey::new_unique();
        let mismatch_account = AccountSharedData::new_ref(100, Account::packed_len(), &mismatch_key);
        let owner_key = Pubkey::new_unique();
        let owner_account = AccountSharedData::new_ref(0, 0, &owner_key);
        let owner2_key = Pubkey::new_unique();
        let owner2_account = AccountSharedData::new_ref(0, 0, &owner2_key);
        let mint_key = Pubkey::new_unique();
        let mint_account = AccountSharedData::new_ref(100, Mint::packed_len(), &mint_key);
        let mint2_key = Pubkey::new_unique();

        // create mint
        process_token_instruction(
            &initialize_mint(&mint_key, &owner_key, None, 2).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
            ],
        ).unwrap();

        // create account
        process_token_instruction(
            &initialize_account(&account_key, &mint_key, &owner_key).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // create another account
        process_token_instruction(
            &initialize_account(&account2_key, &mint_key, &owner_key).unwrap(),
            &[
                (true, true, account2_key, account2_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // create another account
        process_token_instruction(
            &initialize_account(&account3_key, &mint_key, &owner_key).unwrap(),
            &[
                (true, true, account3_key, account3_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // create mismatch account
        process_token_instruction(
            &initialize_account(&mismatch_key, &mint_key, &owner_key).unwrap(),
            &[
                (true, true, mismatch_key, mismatch_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        let mut account = Account::unpack(mismatch_account.try_borrow().unwrap().data()).unwrap();
        account.mint = mint2_key;
        account.pack(mismatch_account.try_borrow_mut().unwrap().data_mut()).unwrap();

        // mint to account
        process_token_instruction(
            &mint_to(&mint_key, &account_key, &owner_key, &[], 1000).unwrap(),
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
                &transfer(&account_key, &account2_key, &owner_key, &[], 1000).unwrap(),
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
                &transfer(&account_key, &mismatch_key, &owner_key, &[], 1000).unwrap(),
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
                &transfer(&account_key, &account2_key, &owner2_key, &[], 1000).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, account2_key, account2_account.clone()),
                    (true, true, owner2_key, owner2_account.clone()),
                ],
            )
        );

        // transfer
        process_token_instruction(
            &transfer(&account_key, &account2_key, &owner_key, &[], 1000).unwrap(),
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
                &transfer(&account_key, &account2_key, &owner_key, &[], 1).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, account2_key, account2_account.clone()),
                    (true, true, owner_key, owner_account.clone()),
                ],
            )
        );

        // transfer half back
        process_token_instruction(
            &transfer(&account2_key, &account_key, &owner_key, &[], 500).unwrap(),
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
                &transfer(&account2_key, &account_key, &owner_key, &[], 1).unwrap(),
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
            &transfer(&account_key, &account2_key, &delegate_key, &[], 100).unwrap(),
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
                &transfer(&account_key, &account2_key, &delegate_key, &[], 100).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, account2_key, account2_account.clone()),
                    (true, true, delegate_key, delegate_account.clone()),
                ],
            )
        );

        // transfer rest
        process_token_instruction(
            &transfer(&account_key, &account2_key, &owner_key, &[], 900).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, account2_key, account2_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();

        // approve delegate
        process_token_instruction(
            &approve(
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
                &transfer(&account_key, &account2_key, &delegate_key, &[], 100).unwrap(),
                &[
                    (true, true, account_key, account_account.clone()),
                    (true, true, account2_key, account2_account.clone()),
                    (true, true, delegate_key, delegate_account.clone()),
                ],
            )
        );
    }

    #[test]
    fn test_initialize_account2() {
        let account_key = Pubkey::new_unique();
        let account_account = AccountSharedData::new_ref(0, Account::packed_len(), &account_key);
        let account2_key = Pubkey::new_unique();
        let account2_account = AccountSharedData::new_ref(0, Account::packed_len(), &account2_key);
        let owner_key = Pubkey::new_unique();
        let owner_account = AccountSharedData::new_ref(0, 0, &owner_key);
        let mint_key = Pubkey::new_unique();
        let mint_account = AccountSharedData::new_ref(0, Mint::packed_len(), &mint_key);

        // create mint
        process_token_instruction(
            &initialize_mint(&mint_key, &owner_key, None, 2).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
            ]).unwrap();

        process_token_instruction(
            &initialize_account(&account_key, &mint_key, &owner_key).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ]).unwrap();

        process_token_instruction(
            &initialize_account2(&account_key, &mint_key, &owner_key).unwrap(),
            &[
                (true, true, account2_key, account2_account.clone()),
                (true, true, mint_key, mint_account.clone()),
            ]).unwrap();

        assert_eq!(account_account.take().data(), account2_account.take().data());
    }

    #[test]
    fn test_multisig() {
        let mint_key = Pubkey::new_unique();
        let mint_account = AccountSharedData::new_ref(0, Mint::packed_len(), &mint_key);

        let multisig_key = Pubkey::new_unique();
        let multisig_account = AccountSharedData::new_ref(42, Multisig::packed_len(), &multisig_key);

        let multisig_delegate_key = Pubkey::new_unique();
        let multisig_delegate_account = AccountSharedData::new_ref(0, Multisig::packed_len(), &multisig_delegate_key);

        let signer_keys = vec![Pubkey::new_unique(); MAX_SIGNERS];
        let signer_key_refs: Vec<&Pubkey> = signer_keys.iter().collect();
        let mut signer_accounts = vec![];
        for i in 0..MAX_SIGNERS {
            signer_accounts.push(AccountSharedData::new_ref(0, 0, &signer_keys[i]));
        }

        // single signer
        process_token_instruction(
            &initialize_multisig(&multisig_key, &[&signer_keys[0]], 1).unwrap(),
            &[
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone())
            ],
        ).unwrap();

        // multiple signers
        process_token_instruction(
            &initialize_multisig(&multisig_key, &signer_key_refs, MAX_SIGNERS as u8).unwrap(),
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
            &initialize_mint(&mint_key, &multisig_key, None, 2).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
            ],
        ).unwrap();

        // create account with multisig owner
        let account_key = Pubkey::new_unique();
        let account_account = AccountSharedData::new_ref(84, Account::packed_len(), &account_key);
        process_token_instruction(
            &initialize_account(&account_key, &mint_key, &multisig_key).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
            ],
        ).unwrap();

        // create another account with multisig owner
        let account2_key = Pubkey::new_unique();
        let account2_account = AccountSharedData::new_ref(0, Account::packed_len(), &account2_key);
        process_token_instruction(
            &initialize_account(&account2_key, &mint_key, &multisig_delegate_key).unwrap(),
            &[
                (true, true, account2_key, account2_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
            ],
        ).unwrap();

        // mint to account
        process_token_instruction(
            &mint_to(&mint_key, &account_key, &multisig_key, &[&signer_keys[0]], 1000).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
                (true, true, account_key, account_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone())
            ],
        ).unwrap();

        // approve
        process_token_instruction(
            &approve(&account_key, &multisig_delegate_key, &multisig_key, &[&signer_keys[0]], 100).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, multisig_delegate_key, multisig_delegate_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone()),
            ],
        ).unwrap();

        // transfer
        process_token_instruction(
            &transfer(&account_key, &account2_key, &multisig_key, &[&signer_keys[0]], 42).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, account2_key, account2_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone()),
            ],
        ).unwrap();

        // transfer via delegate
        process_token_instruction(
            &transfer(&account_key, &account2_key, &multisig_delegate_key, &signer_key_refs, 42).unwrap(),
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
            &mint_to(&mint_key, &account2_key, &multisig_key, &[&signer_keys[0]], 42).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
                (true, true, account2_key, account2_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone())
            ],
        ).unwrap();

        // burn
        process_token_instruction(
            &burn(&account_key, &mint_key, &multisig_key, &[&signer_keys[0]], 42).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, mint_key, mint_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone())
            ],
        ).unwrap();

        // burn via delegate
        process_token_instruction(
            &burn(&account_key, &mint_key, &multisig_delegate_key, &signer_key_refs, 42).unwrap(),
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
        let account3_account = AccountSharedData::new_ref(0, Account::packed_len(), &account3_key);
        let mint2_key = Pubkey::new_unique();
        let mint2_account = AccountSharedData::new_ref(0, Mint::packed_len(), &mint2_key);
        process_token_instruction(
            &initialize_mint(&mint2_key, &multisig_key, Some(&multisig_key), 2).unwrap(),
            &[
                (true, true, mint2_key, mint2_account.clone()),
            ],
        ).unwrap();

        let owner_key = Pubkey::new_unique();
        let owner_account = AccountSharedData::new_ref(0, 0, &owner_key);
        process_token_instruction(
            &initialize_account(&account3_key, &mint2_key, &owner_key).unwrap(),
            &[
                (true, true, account3_key, account3_account.clone()),
                (true, true, mint2_key, mint2_account.clone()),
                (true, true, owner_key, owner_account.clone()),
            ],
        ).unwrap();
        process_token_instruction(
            &mint_to(&mint2_key, &account3_key, &multisig_key, &[&signer_keys[0]], 1000).unwrap(),
            &[
                (true, true, mint2_key, mint2_account.clone()),
                (true, true, account3_key, account3_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone())
            ],
        ).unwrap();
        process_token_instruction(
            &freeze_account(&account3_key, &mint2_key, &multisig_key, &[&signer_keys[0]]).unwrap(),
            &[
                (true, true, account3_key, account3_account.clone()),
                (true, true, mint2_key, mint2_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone())
            ],
        ).unwrap();

        // do SetAuthority on mint
        process_token_instruction(
            &set_authority(&mint_key, Some(&owner_key), AuthorityType::MintTokens, &multisig_key, &[&signer_keys[0]]).unwrap(),
            &[
                (true, true, mint_key, mint_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone())
            ],
        ).unwrap();

        // do SetAuthority on account
        process_token_instruction(
            &set_authority(&account_key, Some(&owner_key), AuthorityType::AccountOwner, &multisig_key, &[&signer_keys[0]]).unwrap(),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, multisig_key, multisig_account.clone()),
                (true, true, signer_keys[0], signer_accounts[0].clone())
            ],
        ).unwrap();
    }

    // TODO: Mundis: add remaining tests
}