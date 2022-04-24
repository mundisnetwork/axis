use std::collections::HashMap;
use mundis_program_runtime::invoke_context::InvokeContext;
use mundis_sdk::keyed_account::{keyed_account_at_index, KeyedAccount, next_keyed_account};
use mundis_sdk::program_utils::limited_deserialize;
use mundis_program_runtime::ic_msg;
use mundis_sdk::account::{ReadableAccount, WritableAccount};
use mundis_sdk::account_utils::State;
use crate::error::{PrintInstructionError, SidechainError};
use mundis_sdk::instruction::InstructionError;
use mundis_sdk::system_instruction;
use crate::Pubkey;
use crate::sc_instruction::ScRegistryInstruction;
use crate::state::{SidechainRecord, SidechainState};

pub fn process_instruction(
    first_instruction_account: usize,
    data: &[u8],
    invoke_context: &mut InvokeContext,
) -> Result<(), InstructionError> {
    Processor::process(first_instruction_account, data, invoke_context)
}

pub struct Processor {}

impl Processor {
    fn process(
        first_instruction_account: usize,
        data: &[u8],
        invoke_context: &mut InvokeContext,
    ) -> Result<(), InstructionError> {
        match limited_deserialize(data)? {
            ScRegistryInstruction::RegisterChain {
                website_url,
                github_url,
                contact_email,
                deposit_lamports: deposit,
            } => {
                ic_msg!(invoke_context, "Instruction: RegisterChain");
                Self::process_register_chain(
                    invoke_context,
                    first_instruction_account,
                    website_url,
                    github_url,
                    contact_email,
                    deposit,
                )
            }
            ScRegistryInstruction::UpvoteChain { chain_id } => {
                Ok(())
            }
            ScRegistryInstruction::DownvoteChain { chain_id } => {
                Ok(())
            }
        }
    }

    pub fn process_register_chain(
        invoke_context: &mut InvokeContext,
        first_instruction_account: usize,
        website_url: Option<String>,
        github_url: Option<String>,
        contact_email: Option<String>,
        deposit: u64,
    ) -> Result<(), InstructionError> {
        let keyed_accounts = invoke_context.get_keyed_accounts()?;
        let fee_payer_account = keyed_account_at_index(keyed_accounts, first_instruction_account)?;
        let owner_account = keyed_account_at_index(keyed_accounts, first_instruction_account + 1)?;
        let chain_account = keyed_account_at_index(keyed_accounts, first_instruction_account + 2)?;

        let sidechain = SidechainRecord::unpack(chain_account.try_account_ref()?.data())
            .unwrap_or(SidechainRecord::default());
        if sidechain.is_initialized {
            ic_msg!(invoke_context, "Chain account already exists");
            return Err(InstructionError::AccountAlreadyInitialized);
        }

        if fee_payer_account.lamports()? < deposit {
            ic_msg!(invoke_context, "Insufficient funds to create a deposit for the new chain");
            return Err(InstructionError::InsufficientFunds);
        }

        let payer_key = *fee_payer_account.signer_key().ok_or_else(|| {
            ic_msg!(invoke_context, "Payer account must be a signer");
            InstructionError::MissingRequiredSignature
        })?;
        let chain_key = *chain_account.signer_key().ok_or_else(|| {
            ic_msg!(invoke_context, "Chain account must be a signer");
            InstructionError::MissingRequiredSignature
        })?;

        let owner_pubkey = *owner_account.unsigned_key();

        invoke_context.native_invoke(
            system_instruction::transfer(&payer_key, &chain_key, deposit),
            &[payer_key],
        )?;

        let keyed_accounts = invoke_context.get_keyed_accounts()?;
        let chain_account = keyed_account_at_index(keyed_accounts, first_instruction_account + 2)?;

        let record = SidechainRecord {
            chain_owner: owner_pubkey,
            website_url,
            github_url,
            contact_email,
            deposit,
            state: SidechainState::Registered,
            vote_deposit: 0,
            registration_time: 0,
            boot_time: 0,
            validator_count: 0,
            total_stake: 0,
            is_initialized: true
        };
        record.pack(chain_account.try_account_ref_mut()?.data_mut())
    }
}
