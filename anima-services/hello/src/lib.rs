extern crate mundis_program;
use mundis_program::{account_info::AccountInfo, entrypoint, entrypoint::ProgramResult, msg, pubkey::Pubkey};

mundis_program::declare_id!("7oXNm4reoW3YzL154P7yx3LQRRjmgsgtHEwmsHgWSvVL");

entrypoint!(process_instruction);
#[allow(clippy::unnecessary_wraps)]
fn process_instruction(
    _program_id: &Pubkey,
    _accounts: &[AccountInfo],
    _instruction_data: &[u8],
) -> ProgramResult {
    msg!("Hello NORMAL program");
    Ok(())
}
