use mundis_program::account_info::AccountInfo;
use mundis_program::instruction::InstructionError;
use mundis_program::program_option::COption;
use mundis_program::pubkey::Pubkey;
use crate::error::TokenError;
use crate::token_instruction::{AuthorityType, TokenInstruction};

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
        let instruction = TokenInstruction::unpack(input)?;

        match instruction {
            TokenInstruction::InitializeMint {
                decimals,
                mint_authority,
                freeze_authority,
            } => {
                ic_msg!(invoke_context, "Instruction: InitializeMint");
                Self::process_initialize_mint(accounts, decimals, mint_authority, freeze_authority)
            }
            TokenInstruction::InitializeAccount => {
                ic_msg!("Instruction: InitializeAccount");
                Self::process_initialize_account(accounts)
            }
            TokenInstruction::InitializeAccount2 { owner } => {
                ic_msg!("Instruction: InitializeAccount2");
                Self::process_initialize_account2(accounts, owner)
            }
            TokenInstruction::InitializeMultisig { m } => {
                ic_msg!("Instruction: InitializeMultisig");
                Self::process_initialize_multisig(accounts, m)
            }
            TokenInstruction::Transfer { amount } => {
                ic_msg!("Instruction: Transfer");
                Self::process_transfer(program_id, accounts, amount, None)
            }
            TokenInstruction::Approve { amount } => {
                ic_msg!("Instruction: Approve");
                Self::process_approve(program_id, accounts, amount, None)
            }
            TokenInstruction::Revoke => {
                ic_msg!("Instruction: Revoke");
                Self::process_revoke(program_id, accounts)
            }
            TokenInstruction::SetAuthority {
                authority_type,
                new_authority,
            } => {
                ic_msg!("Instruction: SetAuthority");
                Self::process_set_authority(program_id, accounts, authority_type, new_authority)
            }
            TokenInstruction::MintTo { amount } => {
                ic_msg!("Instruction: MintTo");
                Self::process_mint_to(program_id, accounts, amount, None)
            }
            TokenInstruction::Burn { amount } => {
                ic_msg!("Instruction: Burn");
                Self::process_burn(program_id, accounts, amount, None)
            }
            TokenInstruction::CloseAccount => {
                ic_msg!("Instruction: CloseAccount");
                Self::process_close_account(program_id, accounts)
            }
            TokenInstruction::FreezeAccount => {
                ic_msg!("Instruction: FreezeAccount");
                Self::process_toggle_freeze_account(program_id, accounts, true)
            }
            TokenInstruction::ThawAccount => {
                ic_msg!("Instruction: ThawAccount");
                Self::process_toggle_freeze_account(program_id, accounts, false)
            }
            TokenInstruction::TransferChecked { amount, decimals } => {
                ic_msg!("Instruction: TransferChecked");
                Self::process_transfer(program_id, accounts, amount, Some(decimals))
            }
            TokenInstruction::ApproveChecked { amount, decimals } => {
                ic_msg!("Instruction: ApproveChecked");
                Self::process_approve(program_id, accounts, amount, Some(decimals))
            }
            TokenInstruction::MintToChecked { amount, decimals } => {
                ic_msg!("Instruction: MintToChecked");
                Self::process_mint_to(program_id, accounts, amount, Some(decimals))
            }
            TokenInstruction::BurnChecked { amount, decimals } => {
                ic_msg!("Instruction: BurnChecked");
                Self::process_burn(program_id, accounts, amount, Some(decimals))
            }
            TokenInstruction::SyncNative => {
                ic_msg!("Instruction: SyncNative");
                Self::process_sync_native(program_id, accounts)
            }
        }
    }

    pub fn process_initialize_mint(
        accounts: &[AccountInfo],
        decimals: u8,
        mint_authority: Pubkey,
        freeze_authority: COption<Pubkey>,
    ) -> Result<(), Err<InstructionError>> {

    }

    fn _process_initialize_account(
        accounts: &[AccountInfo],
        owner: Option<&Pubkey>,
    ) -> Result<(), Err<InstructionError>> {

    }

    /// Processes an [InitializeAccount](enum.TokenInstruction.html) instruction.
    pub fn process_initialize_account(accounts: &[AccountInfo]) -> Result<(), Err<InstructionError>> {
        Self::_process_initialize_account(accounts, None)
    }

    /// Processes an [InitializeAccount2](enum.TokenInstruction.html) instruction.
    pub fn process_initialize_account2(accounts: &[AccountInfo], owner: Pubkey) -> Result<(), Err<InstructionError>> {
        Self::_process_initialize_account(accounts, Some(&owner))
    }

    /// Processes a [InitializeMultisig](enum.TokenInstruction.html) instruction.
    pub fn process_initialize_multisig(accounts: &[AccountInfo], m: u8) -> Result<(), Err<InstructionError>> {

    }

    /// Processes a [Transfer](enum.TokenInstruction.html) instruction.
    pub fn process_transfer(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        amount: u64,
        expected_decimals: Option<u8>,
    ) -> Result<(), Err<InstructionError>> {

    }

    /// Processes an [Approve](enum.TokenInstruction.html) instruction.
    pub fn process_approve(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        amount: u64,
        expected_decimals: Option<u8>,
    ) -> Result<(), Err<InstructionError>> {

    }

    /// Processes an [Revoke](enum.TokenInstruction.html) instruction.
    pub fn process_revoke(program_id: &Pubkey, accounts: &[AccountInfo]) -> Result<(), Err<InstructionError>> {

    }

    pub fn process_set_authority(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        authority_type: AuthorityType,
        new_authority: COption<Pubkey>,
    ) -> Result<(), Err<InstructionError>> {

    }

    /// Processes a [MintTo](enum.TokenInstruction.html) instruction.
    pub fn process_mint_to(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        amount: u64,
        expected_decimals: Option<u8>,
    ) -> Result<(), Err<InstructionError>> {

    }

    /// Processes a [Burn](enum.TokenInstruction.html) instruction.
    pub fn process_burn(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        amount: u64,
        expected_decimals: Option<u8>,
    ) -> Result<(), Err<InstructionError>> {
    }

    /// Processes a [CloseAccount](enum.TokenInstruction.html) instruction.
    pub fn process_close_account(program_id: &Pubkey, accounts: &[AccountInfo]) -> Result<(), Err<InstructionError>> {

    }

    /// Processes a [FreezeAccount](enum.TokenInstruction.html) or a
    /// [ThawAccount](enum.TokenInstruction.html) instruction.
    pub fn process_toggle_freeze_account(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        freeze: bool,
    ) -> Result<(), Err<InstructionError>> {

    }

    /// Processes a [SyncNative](enum.TokenInstruction.html) instruction
    pub fn process_sync_native(program_id: &Pubkey, accounts: &[AccountInfo]) -> Result<(), Err<InstructionError>> {

    }
}


#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use mundis_program::instruction::InstructionError;
    use mundis_program::pubkey::Pubkey;
    use mundis_sdk::account::AccountSharedData;

    fn process_token_instruction(
        instruction_data: &[u8],
        keyed_accounts: &[(bool, bool, Pubkey, Rc<RefCell<AccountSharedData>>)],
    ) -> Result<(), InstructionError> {
        mock_process_instruction(
            &mundis_sdk::token::program::id(),
            Vec::new(),
            instruction_data,
            keyed_accounts,
            super::process_instruction,
        )
    }
}