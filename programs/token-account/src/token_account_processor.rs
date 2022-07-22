use mundis_program_runtime::ic_msg;
use mundis_program_runtime::invoke_context::InvokeContext;
use mundis_sdk::instruction::InstructionError;
use mundis_sdk::keyed_account::{keyed_account_at_index, next_keyed_account};
use mundis_sdk::program_pack::Pack;
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
        let accounts_iter = &mut accounts.iter();
        let token_program_id = mundis_sdk::token::program::id();

        let funder_info = next_keyed_account(accounts_iter)?;
        let associated_token_account_info = next_keyed_account(accounts_iter)?;
        let wallet_account_info = next_keyed_account(accounts_iter)?;
        let token_mint_info = next_keyed_account(accounts_iter)?;

        let (associated_token_address, bump_seed) = get_associated_token_address_and_bump_seed_internal(
            wallet_account_info.unsigned_key(),
            token_mint_info.unsigned_key(),
            program_id,
            &token_program_id
        );

        if associated_token_address != *associated_token_account_info.unsigned_key() {
            ic_msg!(invoke_context, "Error: Associated address does not match seed derivation");
            return Err(InstructionError::InvalidSeeds);
        }

        let associated_token_account_key = *associated_token_account_info.unsigned_key();
        let funder_info_key = *funder_info.unsigned_key();
        let token_mint_info_key = *token_mint_info.unsigned_key();
        let wallet_account_key = *wallet_account_info.unsigned_key();

        let associated_token_account_signer_seeds:&[&[_]] = &[
            &wallet_account_key.to_bytes(),
            &token_program_id.to_bytes(),
            &token_mint_info_key.to_bytes(),
            &[bump_seed]
        ];

        let signers = Pubkey::create_program_address(associated_token_account_signer_seeds, program_id)?;
        let rent = invoke_context.get_sysvar_cache().get_rent()?;

        if associated_token_account_info.lamports()? > 0 {
            let required_lamports = rent
                .minimum_balance(TokenAccount::get_packed_len())
                .max(1)
                .saturating_sub(associated_token_account_info.lamports()?);

            if required_lamports > 0 {
                invoke_context.native_invoke(
                   system_instruction::transfer(&funder_info_key, &associated_token_account_key, required_lamports),
                    &[
                        funder_info_key,
                        associated_token_account_key,
                    ]
                )?;
            }

            invoke_context.native_invoke(
                system_instruction::allocate(&associated_token_account_key, TokenAccount::get_packed_len() as u64),
                &[signers],
            )?;
            invoke_context.native_invoke(
                system_instruction::assign(&associated_token_account_key, &token_program_id),
                &[signers],
            )?;
        } else {
            invoke_context.native_invoke(system_instruction::create_account(
                &funder_info_key,
                &associated_token_account_key,
                rent.minimum_balance(TokenAccount::LEN).max(1),
                TokenAccount::get_packed_len() as u64,
                &token_program_id,
            ), &[signers])?;
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
