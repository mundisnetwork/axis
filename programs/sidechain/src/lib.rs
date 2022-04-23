#![forbid(unsafe_code)]

pub mod error;
pub mod sc_instruction;
pub mod sc_processor;
pub mod state;

use mundis_sdk::instruction::InstructionError;
use mundis_sdk::pubkey::Pubkey;
pub use mundis_sdk::sidechain::program::{check_id, id};

/// Checks that the supplied program ID is the correct one for the native token
pub fn check_program_account(program_id: &Pubkey) -> Result<(), InstructionError> {
    if program_id != &id() {
        return Err(InstructionError::IncorrectProgramId);
    }
    Ok(())
}