use mundis_program::instruction::{AccountMeta, InstructionError};
use mundis_program::pubkey::Pubkey;
use mundis_program::{system_instruction, sysvar};
use mundis_program::native_token::mun_to_lamports;
use mundis_sdk::account::Account;
use mundis_sdk::signature::Keypair;
use mundis_sdk::signer::Signer;
use mundis_sdk::transaction::{Transaction, TransactionError};
use mundis_sidechain_program::sc_instruction::register_chain;
use mundis_sidechain_program::state::SidechainRecord;
use mundis_test_harness::program_test::ProgramTest;

#[tokio::test]
async fn test_register_sidechain() {
    let mut program_test = ProgramTest::new();
    program_test.add_builtin_program("", mundis_sidechain_program::id(), mundis_sidechain_program::sc_processor::process_instruction);

    let chain_keypair = Keypair::new();
    program_test.add_account(chain_keypair.pubkey(), Account::new(0, SidechainRecord::packed_len(), &mundis_sidechain_program::id()));
    let owner_pubkey = Pubkey::new_unique();
    program_test.add_account(owner_pubkey, Account::new(0, 0, &owner_pubkey));

    let (mut banks_client, payer, recent_blockhash) =
        program_test.start().await;

    println!("caller payer={}", payer.pubkey());
    println!("caller chain_pubkey={}", chain_keypair.pubkey());
    println!("caller owner_pubkey={}", owner_pubkey);

    let mut transaction = Transaction::new_with_payer(
        &[
            register_chain(
                &mundis_sdk::sidechain::program::id(),
                &payer.pubkey(),
                &chain_keypair.pubkey(),
                &owner_pubkey,
                Some("https://google.com".to_string()),
                None,
                None,
                mun_to_lamports(1.0),
            ).unwrap(),
        ],
        Some(&payer.pubkey()),
    );

    transaction.sign(&vec![&payer, &chain_keypair], recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();
}