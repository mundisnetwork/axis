#![forbid(unsafe_code)]

pub mod error;
pub mod token_instruction;
pub mod native_mint;
pub mod token_processor;
pub mod state;

use mundis_program::instruction::InstructionError;
use mundis_program::pubkey::Pubkey;

pub use mundis_sdk::token::program::{check_id, id};

/// Convert the UI representation of a token amount (using the decimals field defined in its mint)
/// to the raw amount
pub fn ui_amount_to_amount(ui_amount: f64, decimals: u8) -> u64 {
    (ui_amount * 10_usize.pow(decimals as u32) as f64) as u64
}

/// Convert a raw amount to its UI representation (using the decimals field defined in its mint)
pub fn amount_to_ui_amount(amount: u64, decimals: u8) -> f64 {
    amount as f64 / 10_usize.pow(decimals as u32) as f64
}

/// Checks that the supplied program ID is the correct one for the native token
pub fn check_program_account(token_program_id: &Pubkey) -> Result<(), InstructionError> {
    if token_program_id != &id() {
        return Err(InstructionError::IncorrectProgramId);
    }
    Ok(())
}
