#![cfg_attr(RUSTC_WITH_SPECIALIZATION, feature(min_specialization))]
#![allow(clippy::integer_arithmetic)]

pub mod authorized_voters;
pub mod vote_instruction;
pub mod vote_state;
pub mod vote_transaction;

#[macro_use]
extern crate mundis_metrics;

#[macro_use]
extern crate mundis_frozen_abi_macro;

pub use mundis_sdk::vote::program::{check_id, id};
