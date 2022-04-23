//! Instruction types

use std::collections::HashMap;
use serde_derive::{Deserialize, Serialize};
use mundis_sdk::instruction::{AccountMeta, Instruction, InstructionError};
use mundis_sdk::system_program;
use crate::{check_program_account, Pubkey};
use crate::state::SidechainState;

/// Instructions supported by the sidechain registry program.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum ScRegistryInstruction {
    RegisterChain {
        website_url: Option<String>,
        github_url: Option<String>,
        contact_email: Option<String>,
        deposit_lamports: u64,
    },
    UpvoteChain {
        chain_id: Pubkey,
    },
    DownvoteChain {
        chain_id: Pubkey,
    }
}

pub fn register_chain(
    registry_program_id: &Pubkey,
    payer_pubkey: &Pubkey,
    chain_pubkey: &Pubkey,
    authority_pubkey: &Pubkey,
    website_url: Option<String>,
    github_url: Option<String>,
    contact_email: Option<String>,
    deposit_lamports: u64,
) -> Result<Instruction, InstructionError> {
    check_program_account(registry_program_id)?;

    Ok(Instruction::new_with_bincode(
        *registry_program_id,
        &ScRegistryInstruction::RegisterChain {
            website_url,
            github_url,
            contact_email,
            deposit_lamports
        },
        vec![
            AccountMeta::new(*payer_pubkey, true),
            AccountMeta::new_readonly(*authority_pubkey, false),
            AccountMeta::new(*chain_pubkey, true),
            AccountMeta::new_readonly(system_program::id(), false),
        ]
    ))
}