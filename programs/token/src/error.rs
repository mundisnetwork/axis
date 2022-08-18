//! Error types

use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use thiserror::Error;

use mundis_sdk::decode_error::DecodeError;
use mundis_sdk::instruction::InstructionError;

/// Errors that may be returned by the Token program.
#[derive(Clone, Debug, Eq, Error, FromPrimitive, PartialEq)]
pub enum TokenError {
    // 0
    #[error("Lamport balance below rent-exempt threshold")]
    NotRentExempt,
    /// Insufficient funds for the operation requested.
    #[error("Insufficient funds")]
    InsufficientFunds,
    /// Invalid Mint.
    #[error("Invalid Mint")]
    InvalidMint,
    /// Account not associated with this Mint.
    #[error("Account not associated with this Mint")]
    MintMismatch,
    /// Owner does not match.
    #[error("Owner does not match")]
    OwnerMismatch,

    // 5
    /// This token's supply is fixed and new tokens cannot be minted.
    #[error("Fixed supply")]
    FixedSupply,
    /// The account cannot be initialized because it is already being used.
    #[error("Already in use")]
    AlreadyInUse,
    /// Invalid number of provided signers.
    #[error("Invalid number of provided signers")]
    InvalidNumberOfProvidedSigners,
    /// Invalid number of required signers.
    #[error("Invalid number of required signers")]
    InvalidNumberOfRequiredSigners,
    /// State is uninitialized.
    #[error("State is unititialized")]
    UninitializedState,

    // 10
    /// Instruction does not support native tokens
    #[error("Instruction does not support native tokens")]
    NativeNotSupported,
    /// Non-native account can only be closed if its balance is zero
    #[error("Non-native account can only be closed if its balance is zero")]
    NonNativeHasBalance,
    /// Invalid instruction
    #[error("Invalid instruction")]
    InvalidInstruction,
    /// State is invalid for requested operation.
    #[error("State is invalid for requested operation")]
    InvalidState,
    /// Operation overflowed
    #[error("Operation overflowed")]
    Overflow,

    // 15
    /// Account does not support specified authority type.
    #[error("Account does not support specified authority type")]
    AuthorityTypeNotSupported,
    /// This token mint cannot freeze accounts.
    #[error("This token mint cannot freeze accounts")]
    MintCannotFreeze,
    /// Account is frozen; all account operations will fail
    #[error("Account is frozen")]
    AccountFrozen,
    /// Mint decimals mismatch between the client and mint
    #[error("The provided decimals value different from the Mint decimals")]
    MintDecimalsMismatch,
    /// Instruction does not support non-native tokens
    #[error("Instruction does not support non-native tokens")]
    NonNativeNotSupported,
}

impl From<TokenError> for InstructionError {
    fn from(e: TokenError) -> Self {
        InstructionError::Custom(e as u32)
    }
}

impl<T> DecodeError<T> for TokenError {
    fn type_of() -> &'static str {
        "TokenError"
    }
}

pub trait PrintInstructionError {
    fn print<E>(&self)
        where
            E: 'static + std::error::Error + DecodeError<E> + PrintInstructionError + FromPrimitive;
}

impl PrintInstructionError for InstructionError {
    fn print<E>(&self)
        where
            E: 'static + std::error::Error + DecodeError<E> + PrintInstructionError + FromPrimitive,
    {
        // do nothing
    }
}

impl PrintInstructionError for TokenError {
    fn print<E>(&self)
        where
            E: 'static + std::error::Error + DecodeError<E> + PrintInstructionError + FromPrimitive,
    {
        match self {
            TokenError::NotRentExempt => eprintln!("Error: lamport balance below rent-exempt threshold"),
            TokenError::InsufficientFunds => eprintln!("Error: insufficient funds"),
            TokenError::InvalidMint => eprintln!("Error: Invalid Mint"),
            TokenError::MintMismatch => eprintln!("Error: Account not associated with this Mint"),
            TokenError::OwnerMismatch => eprintln!("Error: owner does not match"),
            TokenError::FixedSupply => eprintln!("Error: the total supply of this token is fixed"),
            TokenError::AlreadyInUse => eprintln!("Error: account or token already in use"),
            TokenError::InvalidNumberOfProvidedSigners => {
                eprintln!("Error: Invalid number of provided signers")
            }
            TokenError::InvalidNumberOfRequiredSigners => {
                eprintln!("Error: Invalid number of required signers")
            }
            TokenError::UninitializedState => eprintln!("Error: State is uninitialized"),
            TokenError::NativeNotSupported => {
                eprintln!("Error: Instruction does not support native tokens")
            }
            TokenError::NonNativeHasBalance => {
                eprintln!("Error: Non-native account can only be closed if its balance is zero")
            }
            TokenError::InvalidInstruction => eprintln!("Error: Invalid instruction"),
            TokenError::InvalidState => eprintln!("Error: Invalid account state for operation"),
            TokenError::Overflow => eprintln!("Error: Operation overflowed"),
            TokenError::AuthorityTypeNotSupported => {
                eprintln!("Error: Account does not support specified authority type")
            }
            TokenError::MintCannotFreeze => eprintln!("Error: This token mint cannot freeze accounts"),
            TokenError::AccountFrozen => eprintln!("Error: Account is frozen"),
            TokenError::MintDecimalsMismatch => {
                eprintln!("Error: decimals different from the Mint decimals")
            }
            TokenError::NonNativeNotSupported => {
                eprintln!("Error: Instruction does not support non-native tokens")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::error::{PrintInstructionError, TokenError};

    fn generate_error() -> Result<(), TokenError> {
        Err(TokenError::AccountFrozen)
    }

    #[test]
    fn test_token_error() {
        if let Err(error) = generate_error() {
            error.print::<TokenError>()
        }
    }
}