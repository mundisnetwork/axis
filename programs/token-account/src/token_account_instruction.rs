//! Program instructions
use mundis_sdk::instruction::{AccountMeta, Instruction};
use mundis_sdk::pubkey::Pubkey;
use serde_derive::{Deserialize, Serialize};
use mundis_sdk::system_program;
use crate::get_associated_token_address;

/// Instructions supported by the AssociatedTokenAccount program
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum AssociatedTokenAccountInstruction {
    /// Creates an associated token account for the given wallet address and token mint
    ///
    ///   0. `[writeable,signer]` Funding account (must be a system account)
    ///   1. `[writeable]` Associated token account address to be created
    ///   2. `[]` Wallet address for the new associated token account
    ///   3. `[]` The token mint for the new associated token account
    Create,
}

/// Creates CreateAssociatedTokenAccount instruction
pub fn create_associated_token_account(
    funding_address: &Pubkey,
    wallet_address: &Pubkey,
    token_mint_address: &Pubkey,
) -> Instruction {
    let associated_account_address =
        get_associated_token_address(wallet_address, token_mint_address);

    Instruction::new_with_bincode(
        mundis_sdk::token_account::program::id(),
        &AssociatedTokenAccountInstruction::Create,
        vec![
            AccountMeta::new(*funding_address, true),
            AccountMeta::new(associated_account_address, false),
            AccountMeta::new_readonly(*wallet_address, false),
            AccountMeta::new_readonly(*token_mint_address, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(mundis_sdk::token::program::id(), false),
        ]
    )
}