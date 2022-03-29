extern crate mundis_program;
use mundis_program::{account_info::AccountInfo, entrypoint, entrypoint::ProgramResult, msg, pubkey::Pubkey};

mundis_program::declare_id!("Dm8s48ofFQCJLyWi3iqCm4h44GzqBn1FoJADvoaSAcSd");

entrypoint!(process_instruction);
#[allow(clippy::unnecessary_wraps)]
fn process_instruction(
    _program_id: &Pubkey,
    _accounts: &[AccountInfo],
    _instruction_data: &[u8],
) -> ProgramResult {
    msg!("Hello upgraded program");
    Ok(())
}
