#![forbid(unsafe_code)]

pub mod error;
pub mod sc_registry_instruction;
pub mod sc_registry_processor;
pub mod state;

use mundis_sdk::instruction::InstructionError;
use mundis_sdk::pubkey::Pubkey;
pub use mundis_sdk::sidechain_registry::program::{check_id, id};
