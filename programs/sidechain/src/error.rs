//! Error types

use std::error::Error;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use thiserror::Error;
use mundis_sdk::decode_error::DecodeError;
use mundis_sdk::instruction::InstructionError;

/// Errors that may be returned by the program.
#[derive(Clone, Debug, Eq, Error, FromPrimitive, PartialEq)]
pub enum SidechainError {
    // 0
    /// The account cannot be initialized because it is already being used.
    #[error("Chain already registered")]
    AlreadyRegistered,
    /// State is invalid for requested operation.
    #[error("State is invalid for requested operation")]
    InvalidState,
}
impl From<SidechainError> for InstructionError {
    fn from(e: SidechainError) -> Self {
        InstructionError::Custom(e as u32)
    }
}
impl<T> DecodeError<T> for SidechainError {
    fn type_of() -> &'static str {
        "ScRegistryError"
    }
}

pub trait PrintInstructionError {
    fn print<E>(&self)
        where
            E: 'static + std::error::Error + DecodeError<E> + PrintInstructionError + FromPrimitive;
}

impl PrintInstructionError for InstructionError {
    fn print<E>(&self) where E: 'static + Error + DecodeError<E> + PrintInstructionError + FromPrimitive {
        match self {
            Self::Custom(error) => {
                if let Some(custom_error) = E::decode_custom_error_to_enum(*error) {
                    custom_error.print::<E>();
                } else {
                    eprintln!("Error: Unknown");
                }
            },
            _ => {}
        }
    }
}