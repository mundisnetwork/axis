use {
    serde::Serialize,
    mundis_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
    },
};

// Return an Instruction that invokes `program_id` with `data` and required
// a signature from `from_pubkey`.
pub fn create_invoke_instruction<T: Serialize>(
    from_pubkey: Pubkey,
    program_id: Pubkey,
    data: &T,
) -> Instruction {
    let account_metas = vec![AccountMeta::new(from_pubkey, true)];
    Instruction::new_with_bincode(program_id, data, account_metas)
}
