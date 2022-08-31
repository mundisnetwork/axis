//! Instruction types

use crate::check_program_account;
use serde_derive::{Deserialize, Serialize};
use mundis_sdk::instruction::{AccountMeta, Instruction, InstructionError};
use mundis_sdk::pubkey::Pubkey;
use mundis_sdk::system_program;

/// Instructions supported by the token-vaule program.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum VaultInstruction {
    /// Initialize a token vault, starts inactivate. Add tokens in subsequent instructions, then activate.
    ///
    /// Accounts expected by this instruction:
    ///  0. `[writable]`  Initialized fractional share mint with 0 tokens in supply, authority on mint must be pda of program with seed [prefix, programid]
    ///  1. `[writable]` Initialized redeem treasury token account with 0 tokens in supply, owner of account must be pda of program like above.
    ///  2. `[writable]` Initialized fraction treasury token account with 0 tokens in supply, owner of account must be pda of program like above.
    ///  3. `[writable]` Uninitialized vault account.
    ///  4. `[]` Authority on the vault.
    ///  5. `[]` Pricing Lookup Address.
    InitVault {
        allow_further_share_creation: bool,
    },

    /// Add a token to a inactive token vault
    ///
    /// Accounts expected by this instruction:
    ///  0. `[writable]` Uninitialized safety deposit box account address (will be created and allocated by this endpoint) Address should be pda with seed of [PREFIX, vault_address, token_mint_address]
    ///  1. `[writable]` Initialized Token account
    ///  2. `[writable]` Initialized Token store account with authority of this program, this will get set on the safety deposit box
    ///  3. `[writable]` Initialized inactive fractionalized token vault
    ///  4. `[signer]` Authority on the vault.
    ///  5. `[signer]` Payer.
    ///  6. `[signer]` Transfer Authority to move desired token amount from token account to safety deposit.
    AddTokenToInactiveVault {
        amount: u64,
    },

    /// Activates the vault, distributing initial shares into the fraction treasury.
    /// Tokens can no longer be removed in this state until Combination.
    ///
    /// Accounts expected by this instruction:
    ///  0. `[writable]` Initialized inactivated fractionalized token vault
    ///  1. `[writable]` Fraction mint
    ///  2. `[writable]` Fraction treasury
    ///  3. `[]` Fraction mint authority for the program - seed of [PREFIX, program_id]"
    ///  4. `[signer]` Authority on the vault.
    ActivateVault {
        number_of_shares: u64,
    },

    /// This act checks the external pricing oracle for permission to combine and the price of the circulating market cap to do so.
    /// If you can afford it, this amount is charged and placed into the redeem treasury for shareholders to redeem at a later time.
    /// The treasury then unlocks into Combine state and you can remove the tokens.
    ///
    /// Accounts expected by this instruction:
    ///  0. `[writable]` Initialized activated token vault
    ///  1. `[writable]` Token account containing your portion of the outstanding fraction shares
    ///  2. `[writable]` Token account of the redeem_treasury mint type that you will pay with
    ///  3. `[writable]` Fraction mint
    ///  4. `[writable]` Fraction treasury account
    ///  5. `[writable]` Redeem treasury account
    ///  6. `[]` New authority on the vault going forward - can be same authority if you want
    ///  7. `[signer]` Authority on the vault
    ///  8. `[signer]` Transfer authority for the token account and outstanding fractional shares account you're transferring from
    ///  9. `[]` PDA-based Burn authority for the fraction treasury account containing the uncirculated shares seed [PREFIX, program_id]
    ///  10. `[]` External pricing lookup address
    CombineVault,
}

/// Creates an `InitVault` instruction.
pub fn create_init_vault_instruction(
    program_id: &Pubkey,
    fraction_mint: &Pubkey,
    redeem_treasury: &Pubkey,
    fraction_treasury: &Pubkey,
    vault: &Pubkey,
    vault_authority: &Pubkey,
    external_price_account: &Pubkey,
    allow_further_share_creation: bool,
) -> Result<Instruction, InstructionError> {
    check_program_account(program_id)?;
    Ok(Instruction::new_with_bincode(
        *program_id,
        &VaultInstruction::InitVault {
            allow_further_share_creation
        },
        vec![
            AccountMeta::new(*fraction_mint, false),
            AccountMeta::new(*redeem_treasury, false),
            AccountMeta::new(*fraction_treasury, false),
            AccountMeta::new(*vault, false),
            AccountMeta::new_readonly(*vault_authority, false),
            AccountMeta::new_readonly(*external_price_account, false),
        ]
    ))
}

/// Creates an `AddTokenToInactiveVault` instruction.
pub fn create_add_token_to_inactive_vault_instruction(
    program_id: &Pubkey,
    safety_deposit_box: &Pubkey,
    token_account: &Pubkey,
    store: &Pubkey,
    vault: &Pubkey,
    vault_authority: &Pubkey,
    payer: &Pubkey,
    transfer_authority: &Pubkey,
    amount: u64,
) -> Result<Instruction, InstructionError> {
    check_program_account(program_id)?;
    Ok(Instruction::new_with_bincode(
        *program_id,
        &VaultInstruction::AddTokenToInactiveVault {
            amount
        },
        vec![
            AccountMeta::new(*safety_deposit_box, false),
            AccountMeta::new(*token_account, false),
            AccountMeta::new(*store, false),
            AccountMeta::new(*vault, false),
            AccountMeta::new(*vault_authority, true),
            AccountMeta::new(*payer, true),
            AccountMeta::new_readonly(*transfer_authority, true),
        ]
    ))
}

pub fn create_activate_vault_instruction(
    program_id: &Pubkey,
    vault: &Pubkey,
    fraction_mint: &Pubkey,
    fraction_treasury: &Pubkey,
    fraction_mint_authority: &Pubkey,
    vault_authority: &Pubkey,
    number_of_shares: u64,
) -> Result<Instruction, InstructionError> {
    check_program_account(program_id)?;
    Ok(Instruction::new_with_bincode(
        *program_id,
        &VaultInstruction::ActivateVault {
            number_of_shares
        },
        vec![
            AccountMeta::new(*vault, false),
            AccountMeta::new(*fraction_mint, false),
            AccountMeta::new(*fraction_treasury, false),
            AccountMeta::new_readonly(*fraction_mint_authority, false),
            AccountMeta::new_readonly(*vault_authority, true),
        ]
    ))
}

pub fn create_combine_vault_instruction(
    program_id: &Pubkey,
    vault: &Pubkey,
    outstanding_share_token_account: &Pubkey,
    paying_token_account: &Pubkey,
    fraction_mint: &Pubkey,
    fraction_treasury: &Pubkey,
    redeem_treasury: &Pubkey,
    new_authority: &Pubkey,
    vault_authority: &Pubkey,
    paying_transfer_authority: &Pubkey,
    uncirculated_burn_authority: &Pubkey,
    external_pricing_account: &Pubkey,
) -> Result<Instruction, InstructionError> {
    check_program_account(program_id)?;
    Ok(Instruction::new_with_bincode(
        *program_id,
        &VaultInstruction::CombineVault,
        vec![
            AccountMeta::new(*vault, false),
            AccountMeta::new(*outstanding_share_token_account, false),
            AccountMeta::new(*paying_token_account, false),
            AccountMeta::new(*fraction_mint, false),
            AccountMeta::new(*fraction_treasury, false),
            AccountMeta::new(*redeem_treasury, false),
            AccountMeta::new(*new_authority, false),
            AccountMeta::new_readonly(*vault_authority, true),
            AccountMeta::new_readonly(*paying_transfer_authority, true),
            AccountMeta::new_readonly(*uncirculated_burn_authority, false),
            AccountMeta::new_readonly(*external_pricing_account, false),
        ]
    ))
}