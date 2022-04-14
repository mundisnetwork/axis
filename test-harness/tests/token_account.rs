use mundis_program::pubkey::Pubkey;
use mundis_sdk::account::Account;
use mundis_sdk::signer::Signer;
use mundis_sdk::transaction::Transaction;
use mundis_test_harness::program_test::ProgramTest;
use mundis_token_account_program::get_associated_token_address;
use mundis_token_account_program::token_account_instruction::create_associated_token_account;
use mundis_token_program::state::Mint;

pub fn program_test(token_mint_address: Pubkey) -> ProgramTest {
    let mut pc = ProgramTest::new();
    pc.add_account(token_mint_address, Account::new(1461600, Mint::packed_len(), &mundis_token_program::id()));
    pc
}

#[tokio::test]
async fn test_associated_token_address() {
    let wallet_address = Pubkey::new_unique();
    let token_mint_address = Pubkey::new_unique();
    let associated_token_address =
        get_associated_token_address(&wallet_address, &token_mint_address);

    let (mut banks_client, payer, recent_blockhash) =
        program_test(token_mint_address).start().await;
    let rent = banks_client.get_rent().await.unwrap();
    let expected_token_account_balance = rent.minimum_balance(mundis_token_program::state::TokenAccount::LEN);

    // Associated account does not exist
    assert_eq!(
        banks_client
            .get_account(associated_token_address)
            .await
            .expect("get_account"),
        None,
    );

    let mut transaction = Transaction::new_with_payer(
        &[create_associated_token_account(
            &payer.pubkey(),
            &wallet_address,
            &token_mint_address,
        )],
        Some(&payer.pubkey()),
    );

    transaction.sign(&[&payer], recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();

    // Associated account now exists
    let associated_account = banks_client
        .get_account(associated_token_address)
        .await
        .expect("get_account")
        .expect("associated_account not none");
    assert_eq!(
        associated_account.data.len(),
        mundis_token_program::state::TokenAccount::LEN
    );
    assert_eq!(associated_account.owner, mundis_token_program::id());
    assert_eq!(associated_account.lamports, expected_token_account_balance);
}
