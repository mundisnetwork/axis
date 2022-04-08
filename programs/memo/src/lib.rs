#![cfg_attr(RUSTC_WITH_SPECIALIZATION, feature(min_specialization))]
#![allow(clippy::integer_arithmetic)]

pub mod memo_instruction;
pub mod memo_processor;

pub use mundis_sdk::memo::program::{check_id, id};