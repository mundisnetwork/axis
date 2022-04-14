use mundis_program_runtime::ic_msg;
use mundis_program_runtime::invoke_context::InvokeContext;
use mundis_sdk::instruction::InstructionError;
use mundis_sdk::keyed_account::{keyed_account_at_index, next_keyed_account};
use mundis_sdk::program_utils::limited_deserialize;
use mundis_sdk::system_instruction;
use mundis_token_program::state::TokenAccount;
use crate::*;
use crate::token_account_instruction::AssociatedTokenAccountInstruction;

pub fn process_instruction(
    first_instruction_account: usize,
    data: &[u8],
    invoke_context: &mut InvokeContext,
) -> Result<(), InstructionError> {
    match limited_deserialize(data)? {
        AssociatedTokenAccountInstruction::Create {} => {
            Processor::process_create_associated_token_account(
                invoke_context,
                first_instruction_account,
            )
        }
    }
}

pub struct Processor;

impl Processor {
    /// Processes CreateAssociatedTokenAccount instruction
    pub fn process_create_associated_token_account(
        invoke_context: &mut InvokeContext,
        first_instruction_account: usize,
    ) -> Result<(), InstructionError> {
        let keyed_accounts = invoke_context.get_keyed_accounts()?;
        let program_id = keyed_account_at_index(keyed_accounts, 0)?.unsigned_key();
        let accounts = &keyed_accounts[first_instruction_account..];

        for acc in keyed_accounts {
            ic_msg!(invoke_context, "Program account = {}", acc.unsigned_key());
        }

        let accounts_iter = &mut accounts.iter();

        let token_program_id = mundis_sdk::token::program::id();
        ic_msg!(invoke_context, "Token program id = {}", token_program_id);

        let funder_info = next_keyed_account(accounts_iter)?;
        let associated_token_account_info = next_keyed_account(accounts_iter)?;
        let wallet_account_info = next_keyed_account(accounts_iter)?;
        let token_mint_info = next_keyed_account(accounts_iter)?;

        let associated_token_address = get_associated_token_address(
            wallet_account_info.unsigned_key(),
            token_mint_info.unsigned_key(),
        );
        if associated_token_address != *associated_token_account_info.unsigned_key() {
            return Err(InstructionError::InvalidSeeds);
        }

        let balance = associated_token_account_info.lamports()?;
        let associated_token_account_key = *associated_token_account_info.unsigned_key();
        let funder_info_key = *funder_info.unsigned_key();
        let token_mint_info_key = *token_mint_info.unsigned_key();
        let wallet_account_key = *wallet_account_info.unsigned_key();

        let associated_token_account_signer_seeds = [
            wallet_account_key,
            token_program_id,
            token_mint_info_key,
            associated_token_address
        ];

        if balance > 0 {
            invoke_context.native_invoke(
                system_instruction::allocate(&associated_token_account_key, TokenAccount::packed_len() as u64),
                &associated_token_account_signer_seeds,
            )?;
            invoke_context.native_invoke(
                system_instruction::assign(&associated_token_account_key, &token_program_id),
                &associated_token_account_signer_seeds,
            )?;
        } else {
            ic_msg!(invoke_context, "funder_info_keyy = {}", funder_info_key);
            ic_msg!(invoke_context, "associated_token_account_key = {}", associated_token_account_key);

            invoke_context.native_invoke(system_instruction::create_account(
                &funder_info_key,
                &associated_token_account_key,
                0,
                TokenAccount::packed_len() as u64,
                &token_program_id,
            ), &associated_token_account_signer_seeds)?;
        }

        invoke_context.native_invoke(
            mundis_token_program::token_instruction::initialize_account2(
                &token_program_id,
                &associated_token_account_key,
                &token_mint_info_key,
                &wallet_account_key,
            )?,
            &[],
        )
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use mundis_program_runtime::invoke_context::{mock_process_instruction_with_sysvars};
    use mundis_program_runtime::sysvar_cache::SysvarCache;
    use mundis_sdk::account::{AccountSharedData, WritableAccount};
    use mundis_sdk::instruction::{Instruction, InstructionError};
    use mundis_sdk::pubkey::Pubkey;
    use mundis_sdk::system_program;
    use mundis_token_program::state::TokenAccount;

    use crate::*;
    use crate::token_account_instruction::create_associated_token_account;

    fn process_ta_instruction(
        instruction: &Instruction,
        keyed_accounts: &[(bool, bool, Pubkey, Rc<RefCell<AccountSharedData>>)],
    ) -> Result<(), InstructionError> {
        mock_process_instruction_with_sysvars(
            &mundis_sdk::token_account::program::id(),
            Vec::new(),
            &instruction.data,
            keyed_accounts,
            &SysvarCache::default(),
            super::process_instruction,
            &[],
        )
    }

    #[test]
    fn test_associated_token_address1() {
        let account_key = Pubkey::new_unique();
        let account_account = AccountSharedData::new_ref(100, TokenAccount::packed_len(), &account_key);
        let wallet_address = Pubkey::new_unique();
        let wallet_account = AccountSharedData::new_ref(0, 0, &wallet_address);
        let token_mint_address = Pubkey::new_unique();
        let token_mint_account = AccountSharedData::new_ref(0, 0, &token_mint_address);
        let associated_token_address =
            get_associated_token_address(&wallet_address, &token_mint_address);
        let associated_token_account = AccountSharedData::new_ref(0, 0, &associated_token_address);
        let system_program_account = AccountSharedData::new_ref(0, 0, &system_program::id());
        system_program_account.borrow_mut().set_executable(true);

        println!("account_key = {}", account_key);
        println!("associated_token_address = {}", associated_token_address);
        println!("wallet_address = {}", wallet_address);
        println!("token_mint_address = {}", token_mint_address);

        process_ta_instruction(
            &create_associated_token_account(
                &account_key,
                &wallet_address,
                &token_mint_address,
            ),
            &[
                (true, true, account_key, account_account.clone()),
                (true, true, associated_token_address, associated_token_account.clone()),
                (true, true, wallet_address, wallet_account.clone()),
                (true, true, token_mint_address, token_mint_account.clone()),
                (true, true, system_program::id(), system_program_account),
            ],
        ).unwrap();
    }
}