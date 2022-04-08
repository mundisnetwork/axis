use std::str::from_utf8;
use mundis_program_runtime::ic_msg;
use mundis_program_runtime::invoke_context::InvokeContext;
use mundis_sdk::instruction::InstructionError;

pub fn process_instruction(
    first_instruction_account: usize,
    data: &[u8],
    invoke_context: &mut InvokeContext,
) -> Result<(), InstructionError> {
    let keyed_accounts = invoke_context.get_keyed_accounts()?;
    let mut missing_required_signature = false;

    for account in keyed_accounts[first_instruction_account..].iter() {
        if let Some(address) = account.signer_key() {
            ic_msg!(invoke_context, "Signed by {:?}", address);
        } else {
            missing_required_signature = true;
        }
    }
    if missing_required_signature {
        return Err(InstructionError::MissingRequiredSignature);
    }
    let memo = from_utf8(data).map_err(|err| {
        ic_msg!(invoke_context, "Invalid UTF-8, from byte {}", err.valid_up_to());
        InstructionError::InvalidInstructionData
    })?;

    ic_msg!(invoke_context, "Memo (len {}): {:?}", memo.len(), memo);
    Ok(())
}


#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use mundis_program_runtime::invoke_context::mock_process_instruction;
    use mundis_sdk::account::AccountSharedData;
    use mundis_sdk::instruction::InstructionError;
    use mundis_sdk::pubkey::Pubkey;

    fn process_memo_instruction(
        instruction_data: &[u8],
        keyed_accounts: &[(bool, bool, Pubkey, Rc<RefCell<AccountSharedData>>)],
    ) -> Result<(), InstructionError> {
        mock_process_instruction(
            &mundis_sdk::memo::program::id(),
            Vec::new(),
            instruction_data,
            keyed_accounts,
            super::process_instruction,
        )
    }

    #[test]
    fn test_utf8_memo() {
        let string = b"letters and such";
        assert_eq!(Ok(()), process_memo_instruction(string, &[]));

        let emoji = "🐆".as_bytes();
        let bytes = [0xF0, 0x9F, 0x90, 0x86];
        assert_eq!(emoji, bytes);
        assert_eq!(Ok(()), process_memo_instruction(emoji, &[]));

        let mut bad_utf8 = bytes;
        bad_utf8[3] = 0xFF; // Invalid UTF-8 byte
        assert_eq!(
            Err(InstructionError::InvalidInstructionData),
            process_memo_instruction(&bad_utf8, &[])
        );
    }

    #[test]
    fn test_signers() {
        let memo = "🐆".as_bytes();
        let pubkey0 = Pubkey::new_unique();
        let pubkey1 = Pubkey::new_unique();
        let pubkey2 = Pubkey::new_unique();
        let account0 = AccountSharedData::new_ref(0, 0, &Pubkey::new_unique());
        let account1 = AccountSharedData::new_ref(0, 0, &Pubkey::new_unique());
        let account2 = AccountSharedData::new_ref(0, 0, &Pubkey::new_unique());

        let signed_account_infos = [
            (true, false, pubkey0, account0.clone()),
            (true, false, pubkey1, account1.clone()),
            (true, false, pubkey2, account2.clone()),
        ];
        assert_eq!(Ok(()), process_memo_instruction(memo, &signed_account_infos));
        assert_eq!(Ok(()), process_memo_instruction(memo, &[]));

        let unsigned_account_infos = [
            (false, false, pubkey0, account0.clone()),
            (false, false, pubkey1, account1.clone()),
            (false, false, pubkey2, account2.clone()),
        ];
        assert_eq!(
            Err(InstructionError::MissingRequiredSignature),
            process_memo_instruction(memo, &unsigned_account_infos)
        );

        let partially_signed_account_infos = [
            (true, false, pubkey0, account0.clone()),
            (false, false, pubkey1, account1.clone()),
            (true, false, pubkey2, account2.clone()),
        ];
        assert_eq!(
            Err(InstructionError::MissingRequiredSignature),
            process_memo_instruction(memo, &partially_signed_account_infos)
        );
    }
}
