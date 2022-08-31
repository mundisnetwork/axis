#![forbid(unsafe_code)]

extern crate core;

use mundis_sdk::instruction::InstructionError;
use mundis_sdk::pubkey::Pubkey;
pub use mundis_sdk::token_vault::program::{check_id, id};

pub mod instruction;
pub mod error;
pub mod processor;
pub mod state;
pub mod utils;

/// Checks that the supplied program ID is the correct one
pub fn check_program_account(program_id: &Pubkey) -> Result<(), InstructionError> {
    if program_id != &id() {
        return Err(InstructionError::IncorrectProgramId);
    }
    Ok(())
}