use mundis_program_runtime::invoke_context::InvokeContext;
use mundis_sdk::keyed_account::keyed_account_at_index;
use mundis_sdk::program_utils::limited_deserialize;
use mundis_program_runtime::ic_msg;
use crate::error::{PrintInstructionError, ScRegistryError};
use mundis_sdk::instruction::InstructionError;
use mundis_sdk::decode_error::DecodeError;

pub fn process_instruction(
    first_instruction_account: usize,
    data: &[u8],
    invoke_context: &mut InvokeContext,
) -> Result<(), InstructionError> {
    Processor::process(first_instruction_account, data, invoke_context)
}

pub struct Processor {}

impl Processor {
    fn process(
        first_instruction_account: usize,
        data: &[u8],
        invoke_context: &mut InvokeContext,
    ) -> Result<(), InstructionError> {
        let keyed_accounts = invoke_context.get_keyed_accounts()?;
        let program_id = keyed_account_at_index(keyed_accounts, 0)?.unsigned_key();
        let accounts = &keyed_accounts[first_instruction_account..];

        match limited_deserialize(data)? {
            _ => {}
        };

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    pub fn test1() {

    }
}