use mundis_program::instruction::{AccountMeta, InstructionError};
use mundis_program::pubkey::Pubkey;
use mundis_program::system_instruction;
use mundis_sdk::account::Account;
use mundis_sdk::signer::Signer;
use mundis_sdk::transaction::{Transaction, TransactionError};
use mundis_test_harness::program_test::ProgramTest;
use mundis_token_account_program::get_associated_token_address;
use mundis_token_account_program::token_account_instruction::create_associated_token_account;
use mundis_token_program::state::{Mint, TokenAccount};
use mundis_token_program::token_instruction::initialize_mint;

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
    let expected_token_account_balance = rent.minimum_balance(TokenAccount::LEN);

    // Associated account does not exist
    assert_eq!(
        banks_client
            .get_account(associated_token_address)
            .await
            .expect("get_account"),
        None,
    );

    let mut transaction = Transaction::new_with_payer(
        &[
            initialize_mint(
                &mundis_token_program::id(),
                &token_mint_address,
                &payer.pubkey(),
                None,
                &"Test Token".to_string(),
                &"TST".to_string(),
                3,
            ).unwrap(),
            create_associated_token_account(
                &payer.pubkey(),
                &wallet_address,
                &token_mint_address,
            )
        ],
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
        TokenAccount::LEN
    );
    assert_eq!(associated_account.owner, mundis_token_program::id());
    assert_eq!(associated_account.lamports, expected_token_account_balance as u64);
}

#[tokio::test]
async fn test_create_with_fewer_lamports() {
    let wallet_address = Pubkey::new_unique();
    let token_mint_address = Pubkey::new_unique();
    let associated_token_address =
        get_associated_token_address(&wallet_address, &token_mint_address);

    let (mut banks_client, payer, recent_blockhash) =
        program_test(token_mint_address).start().await;
    let rent = banks_client.get_rent().await.unwrap();
    let expected_token_account_balance = rent.minimum_balance(TokenAccount::LEN);

    // Transfer lamports into `associated_token_address` before creating it - enough to be
    // rent-exempt for 0 data, but not for an initialized token account
    let mut transaction = Transaction::new_with_payer(
        &[
            system_instruction::transfer(
                &payer.pubkey(),
                &associated_token_address,
                rent.minimum_balance(0),
            ),
            initialize_mint(
                &mundis_token_program::id(),
                &token_mint_address,
                &payer.pubkey(),
                None,
                &"Test Token".to_string(),
                &"TST".to_string(),
                3,
            ).unwrap(),
        ],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer], recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();

    assert_eq!(
        banks_client
            .get_balance(associated_token_address)
            .await
            .unwrap(),
        rent.minimum_balance(0)
    );

    // Check that the program adds the extra lamports
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

    assert_eq!(
        banks_client
            .get_balance(associated_token_address)
            .await
            .unwrap(),
        expected_token_account_balance,
    );
}

#[tokio::test]
async fn test_create_with_excess_lamports() {
    let wallet_address = Pubkey::new_unique();
    let token_mint_address = Pubkey::new_unique();
    let associated_token_address =
        get_associated_token_address(&wallet_address, &token_mint_address);

    let (mut banks_client, payer, recent_blockhash) =
        program_test(token_mint_address).start().await;
    let rent = banks_client.get_rent().await.unwrap();
    let expected_token_account_balance = rent.minimum_balance(TokenAccount::LEN);

    // Transfer 1 lamport into `associated_token_address` before creating it
    let mut transaction = Transaction::new_with_payer(
        &[system_instruction::transfer(
            &payer.pubkey(),
            &associated_token_address,
            expected_token_account_balance + 1,
        )],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer], recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();

    assert_eq!(
        banks_client
            .get_balance(associated_token_address)
            .await
            .unwrap(),
        expected_token_account_balance + 1
    );

    // Check that the program doesn't add any lamports
    let mut transaction = Transaction::new_with_payer(
        &[
            initialize_mint(
                &mundis_token_program::id(),
                &token_mint_address,
                &payer.pubkey(),
                None,
                &"Test Token".to_string(),
                &"TST".to_string(),
                3,
            ).unwrap(),
            create_associated_token_account(
                &payer.pubkey(),
                &wallet_address,
                &token_mint_address,
            )],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer], recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();

    assert_eq!(
        banks_client
            .get_balance(associated_token_address)
            .await
            .unwrap(),
        expected_token_account_balance + 1
    );
}

#[tokio::test]
async fn test_create_account_mismatch() {
    let wallet_address = Pubkey::new_unique();
    let token_mint_address = Pubkey::new_unique();
    let _associated_token_address =
        get_associated_token_address(&wallet_address, &token_mint_address);

    let (mut banks_client, payer, recent_blockhash) =
        program_test(token_mint_address).start().await;

    let mut instruction =
        create_associated_token_account(&payer.pubkey(), &wallet_address, &token_mint_address);
    instruction.accounts[1] = AccountMeta::new(Pubkey::default(), false); // <-- Invalid associated_account_address

    let mut transaction = Transaction::new_with_payer(&[
        initialize_mint(
            &mundis_token_program::id(),
            &token_mint_address,
            &payer.pubkey(),
            None,
            &"Test Token".to_string(),
            &"TST".to_string(),
            3,
        ).unwrap(),
        instruction
    ], Some(&payer.pubkey()));
    transaction.sign(&[&payer], recent_blockhash);
    assert_eq!(
        banks_client
            .process_transaction(transaction)
            .await
            .unwrap_err()
            .unwrap(),
        TransactionError::InstructionError(1, InstructionError::InvalidSeeds)
    );

    let mut instruction =
        create_associated_token_account(&payer.pubkey(), &wallet_address, &token_mint_address);
    instruction.accounts[2] = AccountMeta::new(Pubkey::default(), false); // <-- Invalid wallet_address

    let mut transaction = Transaction::new_with_payer(&[
        initialize_mint(
            &mundis_token_program::id(),
            &token_mint_address,
            &payer.pubkey(),
            None,
            &"Test Token".to_string(),
            &"TST".to_string(),
            3,
        ).unwrap(),
        instruction
    ], Some(&payer.pubkey()));
    transaction.sign(&[&payer], recent_blockhash);
    assert_eq!(
        banks_client
            .process_transaction(transaction)
            .await
            .unwrap_err()
            .unwrap(),
        TransactionError::InstructionError(1, InstructionError::InvalidSeeds)
    );

    let mut instruction =
        create_associated_token_account(&payer.pubkey(), &wallet_address, &token_mint_address);
    instruction.accounts[3] = AccountMeta::new(Pubkey::default(), false); // <-- Invalid token_mint_address

    let mut transaction = Transaction::new_with_payer(&[
        initialize_mint(
            &mundis_token_program::id(),
            &token_mint_address,
            &payer.pubkey(),
            None,
            &"Test Token".to_string(),
            &"TST".to_string(),
            3,
        ).unwrap(),
        instruction,
    ], Some(&payer.pubkey()));
    transaction.sign(&[&payer], recent_blockhash);
    assert_eq!(
        banks_client
            .process_transaction(transaction)
            .await
            .unwrap_err()
            .unwrap(),
        TransactionError::InstructionError(1, InstructionError::InvalidSeeds)
    );
}
